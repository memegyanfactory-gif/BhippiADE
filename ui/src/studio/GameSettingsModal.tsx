import { useState } from "react";
import type { GameSettingsData } from "./types";

interface GameSettingsModalProps {
  open: boolean;
  onClose: () => void;
  initialSettings?: Partial<GameSettingsData>;
  onSave?: (settings: GameSettingsData) => void;
}

const DEFAULT_SETTINGS: GameSettingsData = {
  title: "Demo Platformer Game",
  description: "A cozy 3D platformer featuring a squishy glowing jelly slime hero and moving wooden islands.",
  tags: ["3D", "Platformer", "Cozy", "Godot4"],
  posterPath: "res://assets/poster.png",
  webExportDir: "build/web",
  includeCredits: true,
  windowWidth: 1280,
  windowHeight: 720,
};

export function GameSettingsModal({
  open,
  onClose,
  initialSettings,
  onSave,
}: GameSettingsModalProps) {
  const [activeTab, setActiveTab] = useState<"general" | "publish" | "toml">("general");
  const [settings, setSettings] = useState<GameSettingsData>({
    ...DEFAULT_SETTINGS,
    ...initialSettings,
  });
  const [tagInput, setTagInput] = useState("");
  const [tomlText, setTomlText] = useState(() =>
    `[game]
name = "${settings.title}"
description = "${settings.description}"
tags = [${settings.tags.map((t) => `"${t}"`).join(", ")}]
poster = "${settings.posterPath}"
width = ${settings.windowWidth}
height = ${settings.windowHeight}

[godot]
version_pin = "4.7.1-stable"
main_scene = "res://scene/main.tscn"

[publish]
web_dir = "${settings.webExportDir}"
include_credits = ${settings.includeCredits}
`
  );
  const [tomlError, setTomlError] = useState<string | null>(null);

  if (!open) return null;

  const handleAddTag = () => {
    const trimmed = tagInput.trim();
    if (trimmed && !settings.tags.includes(trimmed)) {
      setSettings((s) => ({ ...s, tags: [...s.tags, trimmed] }));
      setTagInput("");
    }
  };

  const handleRemoveTag = (tag: string) => {
    setSettings((s) => ({ ...s, tags: s.tags.filter((t) => t !== tag) }));
  };

  const handleSave = () => {
    if (activeTab === "toml") {
      // Basic TOML validation: check for required sections
      if (!tomlText.includes("[game]") || !tomlText.includes("[godot]")) {
        setTomlError("Validation error: missing required [game] or [godot] table in manifest");
        return;
      }
    }
    onSave?.(settings);
    onClose();
  };

  return (
    <div
      style={{
        position: "fixed",
        inset: 0,
        zIndex: 9999,
        background: "rgba(0,0,0,0.65)",
        backdropFilter: "blur(8px)",
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
      }}
      onClick={onClose}
    >
      <div
        style={{
          width: "560px",
          maxHeight: "85vh",
          background: "var(--studio-surface-translucent)",
          backdropFilter: "blur(30px)",
          border: "1px solid var(--studio-border)",
          borderRadius: "12px",
          boxShadow: "0 20px 50px rgba(0,0,0,0.6)",
          display: "flex",
          flexDirection: "column",
          overflow: "hidden",
        }}
        onClick={(e) => e.stopPropagation()}
        role="dialog"
        aria-label="Project Game Settings"
      >
        {/* Header */}
        <header
          style={{
            height: "52px",
            padding: "0 20px",
            display: "flex",
            alignItems: "center",
            justifyContent: "space-between",
            borderBottom: "1px solid var(--studio-border)",
          }}
        >
          <div style={{ display: "flex", alignItems: "center", gap: "8px" }}>
            <span style={{ fontSize: "16px" }}>⚙️</span>
            <span style={{ fontSize: "14px", fontWeight: 700, color: "#ffffff" }}>
              Project &amp; Game Settings
            </span>
          </div>
          <button
            type="button"
            className="studio-chat-close"
            onClick={onClose}
            title="Close Settings"
          >
            ✕
          </button>
        </header>

        {/* Tab Buttons */}
        <div style={{ display: "flex", borderBottom: "1px solid var(--studio-border)", padding: "0 16px" }}>
          {[
            { id: "general", label: "General [game]" },
            { id: "publish", label: "Publish & Window" },
            { id: "toml", label: "Manifest TOML" },
          ].map((tab) => (
            <button
              key={tab.id}
              type="button"
              onClick={() => setActiveTab(tab.id as any)}
              style={{
                padding: "10px 14px",
                background: "none",
                border: "none",
                borderBottom: activeTab === tab.id ? "2px solid var(--studio-accent)" : "2px solid transparent",
                color: activeTab === tab.id ? "#ffffff" : "var(--studio-text-muted)",
                fontSize: "12.5px",
                fontWeight: activeTab === tab.id ? 600 : 400,
                cursor: "pointer",
              }}
            >
              {tab.label}
            </button>
          ))}
        </div>

        {/* Content */}
        <div style={{ flex: 1, overflowY: "auto", padding: "20px" }}>
          {activeTab === "general" && (
            <div style={{ display: "flex", flexDirection: "column", gap: "14px" }}>
              <div>
                <label style={{ display: "block", fontSize: "11.5px", color: "var(--studio-text-muted)", marginBottom: "4px" }}>
                  Game Title
                </label>
                <input
                  type="text"
                  value={settings.title}
                  onChange={(e) => setSettings((s) => ({ ...s, title: e.target.value }))}
                  style={{
                    width: "100%",
                    background: "rgba(255,255,255,0.06)",
                    border: "1px solid var(--studio-border)",
                    borderRadius: "6px",
                    padding: "6px 10px",
                    color: "#fff",
                    fontSize: "12.5px",
                  }}
                />
              </div>

              <div>
                <label style={{ display: "block", fontSize: "11.5px", color: "var(--studio-text-muted)", marginBottom: "4px" }}>
                  Description
                </label>
                <textarea
                  rows={3}
                  value={settings.description}
                  onChange={(e) => setSettings((s) => ({ ...s, description: e.target.value }))}
                  style={{
                    width: "100%",
                    background: "rgba(255,255,255,0.06)",
                    border: "1px solid var(--studio-border)",
                    borderRadius: "6px",
                    padding: "6px 10px",
                    color: "#fff",
                    fontSize: "12px",
                    resize: "vertical",
                  }}
                />
              </div>

              <div>
                <label style={{ display: "block", fontSize: "11.5px", color: "var(--studio-text-muted)", marginBottom: "4px" }}>
                  Genre Tags
                </label>
                <div style={{ display: "flex", flexWrap: "wrap", gap: "6px", marginBottom: "6px" }}>
                  {settings.tags.map((tag) => (
                    <span
                      key={tag}
                      style={{
                        display: "inline-flex",
                        alignItems: "center",
                        gap: "4px",
                        background: "rgba(255, 119, 0, 0.15)",
                        border: "1px solid rgba(255, 119, 0, 0.3)",
                        color: "#fff",
                        padding: "2px 8px",
                        borderRadius: "12px",
                        fontSize: "11px",
                      }}
                    >
                      {tag}
                      <button
                        type="button"
                        onClick={() => handleRemoveTag(tag)}
                        style={{ background: "none", border: "none", color: "#ff4d4f", cursor: "pointer", fontSize: "10px", padding: 0 }}
                      >
                        ✕
                      </button>
                    </span>
                  ))}
                </div>
                <div style={{ display: "flex", gap: "6px" }}>
                  <input
                    type="text"
                    placeholder="Add tag (e.g. Roguelike)..."
                    value={tagInput}
                    onChange={(e) => setTagInput(e.target.value)}
                    onKeyDown={(e) => e.key === "Enter" && handleAddTag()}
                    style={{
                      flex: 1,
                      background: "rgba(255,255,255,0.06)",
                      border: "1px solid var(--studio-border)",
                      borderRadius: "6px",
                      padding: "5px 10px",
                      color: "#fff",
                      fontSize: "12px",
                    }}
                  />
                  <button
                    type="button"
                    onClick={handleAddTag}
                    className="studio-btn studio-btn-secondary"
                    style={{ padding: "5px 12px", fontSize: "11.5px" }}
                  >
                    + Add
                  </button>
                </div>
              </div>

              <div>
                <label style={{ display: "block", fontSize: "11.5px", color: "var(--studio-text-muted)", marginBottom: "4px" }}>
                  Poster Image Path
                </label>
                <input
                  type="text"
                  value={settings.posterPath}
                  onChange={(e) => setSettings((s) => ({ ...s, posterPath: e.target.value }))}
                  style={{
                    width: "100%",
                    background: "rgba(255,255,255,0.06)",
                    border: "1px solid var(--studio-border)",
                    borderRadius: "6px",
                    padding: "6px 10px",
                    color: "#fff",
                    fontSize: "12px",
                    fontFamily: "monospace",
                  }}
                />
              </div>
            </div>
          )}

          {activeTab === "publish" && (
            <div style={{ display: "flex", flexDirection: "column", gap: "14px" }}>
              <div>
                <label style={{ display: "block", fontSize: "11.5px", color: "var(--studio-text-muted)", marginBottom: "4px" }}>
                  Web Export Output Directory
                </label>
                <input
                  type="text"
                  value={settings.webExportDir}
                  onChange={(e) => setSettings((s) => ({ ...s, webExportDir: e.target.value }))}
                  style={{
                    width: "100%",
                    background: "rgba(255,255,255,0.06)",
                    border: "1px solid var(--studio-border)",
                    borderRadius: "6px",
                    padding: "6px 10px",
                    color: "#fff",
                    fontSize: "12px",
                    fontFamily: "monospace",
                  }}
                />
              </div>

              <div style={{ display: "flex", gap: "12px" }}>
                <div style={{ flex: 1 }}>
                  <label style={{ display: "block", fontSize: "11.5px", color: "var(--studio-text-muted)", marginBottom: "4px" }}>
                    Viewport Width
                  </label>
                  <input
                    type="number"
                    value={settings.windowWidth}
                    onChange={(e) => setSettings((s) => ({ ...s, windowWidth: parseInt(e.target.value) }))}
                    style={{
                      width: "100%",
                      background: "rgba(255,255,255,0.06)",
                      border: "1px solid var(--studio-border)",
                      borderRadius: "6px",
                      padding: "6px 10px",
                      color: "#fff",
                      fontSize: "12px",
                    }}
                  />
                </div>
                <div style={{ flex: 1 }}>
                  <label style={{ display: "block", fontSize: "11.5px", color: "var(--studio-text-muted)", marginBottom: "4px" }}>
                    Viewport Height
                  </label>
                  <input
                    type="number"
                    value={settings.windowHeight}
                    onChange={(e) => setSettings((s) => ({ ...s, windowHeight: parseInt(e.target.value) }))}
                    style={{
                      width: "100%",
                      background: "rgba(255,255,255,0.06)",
                      border: "1px solid var(--studio-border)",
                      borderRadius: "6px",
                      padding: "6px 10px",
                      color: "#fff",
                      fontSize: "12px",
                    }}
                  />
                </div>
              </div>

              <label style={{ display: "flex", alignItems: "center", gap: "8px", fontSize: "12px", color: "#fff", cursor: "pointer", marginTop: "6px" }}>
                <input
                  type="checkbox"
                  checked={settings.includeCredits}
                  onChange={(e) => setSettings((s) => ({ ...s, includeCredits: e.target.checked }))}
                  style={{ accentColor: "var(--studio-accent)" }}
                />
                <span>Include AI Attribution &amp; Credits on Web Export</span>
              </label>
            </div>
          )}

          {activeTab === "toml" && (
            <div>
              <div style={{ fontSize: "11px", color: "var(--studio-text-muted)", marginBottom: "6px" }}>
                Edit Bhippi.game.toml directly:
              </div>
              <textarea
                rows={12}
                value={tomlText}
                onChange={(e) => {
                  setTomlText(e.target.value);
                  setTomlError(null);
                }}
                style={{
                  width: "100%",
                  background: "#12151c",
                  border: tomlError ? "1px solid #ff4d4f" : "1px solid var(--studio-border)",
                  borderRadius: "6px",
                  padding: "8px 10px",
                  color: "#dcdcaa",
                  fontFamily: "monospace",
                  fontSize: "11.5px",
                  lineHeight: "1.45",
                }}
              />
              {tomlError && (
                <div style={{ color: "#ff4d4f", fontSize: "11px", marginTop: "4px" }}>
                  {tomlError}
                </div>
              )}
            </div>
          )}
        </div>

        {/* Footer */}
        <footer
          style={{
            height: "56px",
            padding: "0 20px",
            display: "flex",
            alignItems: "center",
            justifyContent: "flex-end",
            gap: "10px",
            borderTop: "1px solid var(--studio-border)",
            background: "rgba(0,0,0,0.2)",
          }}
        >
          <button
            type="button"
            className="studio-btn studio-btn-secondary"
            onClick={onClose}
            style={{ padding: "6px 14px", fontSize: "12px" }}
          >
            Cancel
          </button>
          <button
            type="button"
            className="studio-btn studio-btn-primary"
            onClick={handleSave}
            style={{ padding: "6px 18px", fontSize: "12px", fontWeight: 600 }}
          >
            Save Settings
          </button>
        </footer>
      </div>
    </div>
  );
}
