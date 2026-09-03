import { useEffect, useRef, useState } from "react";
import logo from "../assets/logo.png";
import type { ProjectSummary } from "../lib/ipc";
import {
  IconBolt,
  IconCheck,
  IconChevronDown,
  IconSearch,
  IconSparkle,
  IconTerminal,
} from "./icons";

export interface ChatWelcomeProps {
  project?: ProjectSummary | null;
  projects?: ProjectSummary[];
  onSelectProject?: (p: ProjectSummary) => void;
  onSelectPrompt: (prompt: string) => void;
}

interface WelcomeAction {
  id: string;
  label: string;
  prompt: string;
  icon: (props: { size?: number; className?: string }) => JSX.Element;
}

// GAD-016: the four starters are game work, because every project here is a game.
const WELCOME_ACTIONS: WelcomeAction[] = [
  {
    id: "build",
    label: "Build a top-down dungeon crawler",
    prompt: "Build a top-down dungeon crawler",
    icon: IconBolt,
  },
  {
    id: "hud",
    label: "Add a health bar to the HUD",
    prompt: "Add a health bar to the HUD",
    icon: IconSparkle,
  },
  {
    id: "world",
    label: "Make the sky stormy and dim the sun",
    prompt: "Make the sky stormy and dim the sun",
    icon: IconSearch,
  },
  {
    id: "playtest",
    label: "Playtest level 1 and report what breaks",
    prompt: "Playtest level 1 and report what breaks",
    icon: IconTerminal,
  },
];

export function ChatWelcome({
  project,
  projects = [],
  onSelectProject,
  onSelectPrompt,
}: ChatWelcomeProps) {
  const [isMenuOpen, setIsMenuOpen] = useState(false);
  const menuRef = useRef<HTMLSpanElement | null>(null);

  // Close dropdown on click outside or escape key
  useEffect(() => {
    if (!isMenuOpen) return;

    const handlePointerDown = (e: PointerEvent) => {
      if (menuRef.current && !menuRef.current.contains(e.target as Node)) {
        setIsMenuOpen(false);
      }
    };

    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        setIsMenuOpen(false);
      }
    };

    window.addEventListener("pointerdown", handlePointerDown, true);
    window.addEventListener("keydown", handleKeyDown);
    return () => {
      window.removeEventListener("pointerdown", handlePointerDown, true);
      window.removeEventListener("keydown", handleKeyDown);
    };
  }, [isMenuOpen]);

  const projectName = project?.name?.trim() || "this project";

  return (
    <div className="chat-welcome minimal">
      {/* SPA-501: the mark sits in the middle of the empty space; the question hangs under it. */}
      <img src={logo} className="chat-welcome-logo" alt="" draggable={false} />
      {/* Clean Minimal Title */}
      <div className="chat-welcome-header">
        <h1 className="chat-welcome-title">
          <span>What should we build in </span>
          <span className="chat-welcome-project-anchor" ref={menuRef}>
            <button
              type="button"
              className={`chat-welcome-project-trigger${isMenuOpen ? " active" : ""}`}
              onClick={() => setIsMenuOpen((prev) => !prev)}
              title={project?.path ? `Current workspace: ${project.path}` : "Select workspace"}
              aria-expanded={isMenuOpen}
              aria-haspopup="true"
            >
              <span className="chat-welcome-project-name">{projectName}</span>
              <IconChevronDown size={13} className="chat-welcome-chevron" />
            </button>

            {isMenuOpen && (
              <div className="chat-welcome-project-menu" role="menu">
                <div className="chat-welcome-menu-header">
                  <span>Workspaces</span>
                  {projects.length > 0 && (
                    <span className="chat-welcome-menu-count">{projects.length}</span>
                  )}
                </div>
                <div className="chat-welcome-menu-list">
                  {projects.length > 0 ? (
                    projects.map((p) => {
                      const isCurrent = p.path === project?.path;
                      return (
                        <button
                          key={p.path}
                          type="button"
                          className={`chat-welcome-menu-item${isCurrent ? " is-current" : ""}`}
                          onClick={() => {
                            onSelectProject?.(p);
                            setIsMenuOpen(false);
                          }}
                          role="menuitem"
                        >
                          <div className="chat-welcome-menu-item-info">
                            <span className="chat-welcome-menu-item-name">{p.name}</span>
                            <span className="chat-welcome-menu-item-path" title={p.path}>
                              {p.path}
                            </span>
                          </div>
                          {isCurrent && (
                            <span className="chat-welcome-menu-item-badge">
                              <IconCheck size={12} />
                            </span>
                          )}
                        </button>
                      );
                    })
                  ) : (
                    <div className="chat-welcome-menu-empty">
                      <span>Current: {project?.path ?? "Default workspace"}</span>
                    </div>
                  )}
                </div>
              </div>
            )}
          </span>
          <span> ?</span>
        </h1>
      </div>

      {/* Sleek Minimal Action Pills */}
      <div className="chat-welcome-minimal-actions" role="group" aria-label="Suggested starters">
        {WELCOME_ACTIONS.map((action) => {
          const Icon = action.icon;
          return (
            <button
              key={action.id}
              type="button"
              className="chat-welcome-minimal-btn"
              onClick={() => onSelectPrompt(action.prompt)}
              title={`Ask: "${action.prompt}"`}
            >
              <span className="chat-welcome-minimal-icon" aria-hidden="true">
                <Icon size={14} />
              </span>
              <span className="chat-welcome-minimal-label">{action.label}</span>
            </button>
          );
        })}
      </div>
    </div>
  );
}
