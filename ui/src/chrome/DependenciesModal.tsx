import { useEffect, useState } from "react";
import type { SystemDependenciesStatus } from "../lib/ipc";
import { api } from "../lib/api";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { IconDownload, IconRefresh } from "../components/icons";

interface DependenciesModalProps {
  open: boolean;
  onClose: () => void;
  onOpenSettings?: () => void;
}

export function DependenciesModal({ open, onClose, onOpenSettings }: DependenciesModalProps) {
  const [deps, setDeps] = useState<SystemDependenciesStatus | null>(null);
  const [loading, setLoading] = useState(false);
  const [installingGodot, setInstallingGodot] = useState(false);
  const [installError, setInstallError] = useState<string | null>(null);
  const [dontShowAgain, setDontShowAgain] = useState(() => {
    try {
      return localStorage.getItem("bhippi-dismiss-dep-setup") === "true";
    } catch {
      return false;
    }
  });

  const refreshDeps = async () => {
    setLoading(true);
    setInstallError(null);
    try {
      const report = await api.checkSystemDependencies();
      setDeps(report);
    } catch {
      // Ignored if offline
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    if (open) {
      void refreshDeps();
    }
  }, [open]);

  if (!open) return null;

  const handleAutoInstallGodot = async () => {
    setInstallingGodot(true);
    setInstallError(null);
    try {
      await api.downloadAndInstallGodot();
      await refreshDeps();
    } catch (err) {
      setInstallError(err instanceof Error ? err.message : String(err));
    } finally {
      setInstallingGodot(false);
    }
  };

  const handleLocateGodot = async () => {
    try {
      const selected = await openDialog({
        multiple: false,
        directory: false,
        title: "Locate Godot 4 Executable",
        filters: [{ name: "Godot Binary", extensions: ["exe", "bin", ""] }],
      });
      if (typeof selected === "string" && selected.trim()) {
        await api.setGodotPath(selected.trim());
        await refreshDeps();
      }
    } catch (err) {
      setInstallError(err instanceof Error ? err.message : String(err));
    }
  };

  const handleToggleDontShow = (checked: boolean) => {
    setDontShowAgain(checked);
    try {
      localStorage.setItem("bhippi-dismiss-dep-setup", checked ? "true" : "false");
    } catch {}
  };

  const godotInstalled = Boolean(deps?.godot_installed);
  const templatesInstalled = Boolean(deps?.templates_installed);
  const gitInstalled = Boolean(deps?.git_installed);
  const providerReady = Boolean(deps?.provider_ready);

  return (
    <div className="deps-modal-backdrop" role="dialog" aria-modal="true" aria-labelledby="deps-title">
      <div className="deps-modal-card">
        {/* Header */}
        <div className="deps-modal-header">
          <div className="deps-header-title-wrap">
            <div className="deps-badge-icon">⚙</div>
            <div>
              <h2 id="deps-title" className="deps-header-title">Setup Bhippi Dependencies</h2>
              <p className="deps-header-sub">
                Bhippi builds native Godot 4 games. Review and download the required engine & tools below.
              </p>
            </div>
          </div>
          <button
            type="button"
            className="deps-close-btn"
            onClick={onClose}
            aria-label="Close setup modal"
            title="Close"
          >
            ✕
          </button>
        </div>

        {/* Content Body */}
        <div className="deps-modal-body">
          {installError && (
            <div className="deps-error-banner" role="alert">
              <strong>Installation Error:</strong> {installError}
            </div>
          )}

          {/* 1. Godot Engine */}
          <div className={`dep-card${godotInstalled ? " ready" : " needed"}`}>
            <div className="dep-card-header">
              <div className="dep-title-group">
                <span className="dep-icon">🎮</span>
                <div>
                  <h3 className="dep-name">Godot 4 Engine (v4.7.1-stable)</h3>
                  <span className="dep-role required">Essential (Required)</span>
                </div>
              </div>
              <div className={`dep-status-pill${godotInstalled ? " success" : " warning"}`}>
                {godotInstalled ? `✓ Ready (v${deps?.godot_version})` : "✕ Not Installed"}
              </div>
            </div>

            <p className="dep-description">
              The native Godot runtime powers the embedded 3D studio viewport, real-time game playtesting, and GDScript type checks.
            </p>

            {godotInstalled ? (
              <div className="dep-installed-path">
                <span className="dep-path-lbl">Executable:</span>
                <code>{deps?.godot_path}</code>
              </div>
            ) : (
              <div className="dep-action-toolbar">
                <button
                  type="button"
                  className="btn-dep-primary"
                  onClick={handleAutoInstallGodot}
                  disabled={installingGodot}
                >
                  {installingGodot ? (
                    <>
                      <IconRefresh size={14} className="spin-icon" /> Downloading & Extracting...
                    </>
                  ) : (
                    <>
                      <IconDownload size={14} /> ⚡ 1-Click Auto Install Godot 4.7.1
                    </>
                  )}
                </button>
                <button
                  type="button"
                  className="btn-dep-secondary"
                  onClick={handleLocateGodot}
                  disabled={installingGodot}
                >
                  📁 Locate Existing Binary…
                </button>
                {deps?.godot_offer_url && (
                  <button
                    type="button"
                    className="btn-dep-link"
                    onClick={() => void api.openExternalUrl(deps.godot_offer_url)}
                  >
                    🔗 Official Website Download
                  </button>
                )}
              </div>
            )}
          </div>

          {/* 2. Export Templates */}
          <div className={`dep-card${templatesInstalled ? " ready" : " neutral"}`}>
            <div className="dep-card-header">
              <div className="dep-title-group">
                <span className="dep-icon">📦</span>
                <div>
                  <h3 className="dep-name">Web & Desktop Export Templates</h3>
                  <span className="dep-role recommended">Recommended</span>
                </div>
              </div>
              <div className={`dep-status-pill${templatesInstalled ? " success" : " neutral"}`}>
                {templatesInstalled ? "✓ Installed" : "○ Optional"}
              </div>
            </div>
            <p className="dep-description">
              Required for instant in-browser game preview in the workbench and packaging standalone web or desktop releases.
            </p>
            {!templatesInstalled && (
              <div className="dep-action-toolbar">
                <button
                  type="button"
                  className="btn-dep-secondary"
                  onClick={() =>
                    void api.openExternalUrl(
                      "https://github.com/godotengine/godot/releases/download/4.7.1-stable/Godot_v4.7.1-stable_export_templates.tpz"
                    )
                  }
                >
                  Download Official Templates (.tpz)
                </button>
              </div>
            )}
          </div>

          {/* 3. AI Providers */}
          <div className={`dep-card${providerReady ? " ready" : " neutral"}`}>
            <div className="dep-card-header">
              <div className="dep-title-group">
                <span className="dep-icon">🧠</span>
                <div>
                  <h3 className="dep-name">AI Provider (Claude Code / Anthropic / OpenAI)</h3>
                  <span className="dep-role recommended">Recommended</span>
                </div>
              </div>
              <div className={`dep-status-pill${providerReady ? " success" : " warning"}`}>
                {providerReady ? `✓ Connected (${deps?.active_provider})` : "○ Offline Demo Mode"}
              </div>
            </div>
            <p className="dep-description">
              Powers automated game design, scene manipulation, GDScript writing, and real-time game bug fixes.
            </p>
            <div className="dep-action-toolbar">
              <button
                type="button"
                className="btn-dep-secondary"
                onClick={() => {
                  onClose();
                  onOpenSettings?.();
                }}
              >
                ⚙ Configure AI Providers in Settings…
              </button>
            </div>
          </div>

          {/* 4. Git Version Control */}
          <div className={`dep-card${gitInstalled ? " ready" : " neutral"}`}>
            <div className="dep-card-header">
              <div className="dep-title-group">
                <span className="dep-icon">🌿</span>
                <div>
                  <h3 className="dep-name">Git Version Control</h3>
                  <span className="dep-role recommended">Recommended</span>
                </div>
              </div>
              <div className={`dep-status-pill${gitInstalled ? " success" : " warning"}`}>
                {gitInstalled ? `✓ Ready` : "✕ Missing"}
              </div>
            </div>
            <p className="dep-description">
              Enables transaction journals, automated project checkpoints, undo, and auto-updates from GitHub.
            </p>
          </div>
        </div>

        {/* Footer */}
        <div className="deps-modal-footer">
          <label className="deps-dismiss-label">
            <input
              type="checkbox"
              checked={dontShowAgain}
              onChange={(e) => handleToggleDontShow(e.target.checked)}
            />
            <span>Don't show automatically on launch</span>
          </label>
          <div className="deps-footer-actions">
            <button
              type="button"
              className="btn-deps-refresh"
              onClick={() => void refreshDeps()}
              disabled={loading}
              title="Re-check dependencies on system"
            >
              <IconRefresh size={13} className={loading ? "spin-icon" : ""} /> Refresh
            </button>
            <button
              type="button"
              className="btn-deps-finish"
              onClick={onClose}
            >
              {godotInstalled ? "Continue to Engine" : "Continue Anyway"}
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
