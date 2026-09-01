import { useEffect, useState, type ReactNode } from "react";
import type { ProjectSummary, ProjectTool, ToolAvailability } from "../lib/ipc";
import { api } from "../lib/api";
import { clipPath } from "../lib/format";
import {
  IconBrowser,
  IconChevronDown,
  IconCode,
  IconEditor,
  IconExternal,
  IconGitBranch,
  IconGitMerge,
  IconPanelRight,
  IconRules,
  IconTerminal,
} from "../components/icons";
import type { WorkbenchMode } from "../workbench/ModeSwitch";

/** External tools get their own glyph so the drop-up is not four identical rows. */
const TOOL_ICONS: Record<ProjectTool, (props: { size?: number }) => JSX.Element> = {
  vs_code: IconCode,
  cursor: IconCode,
  antigravity: IconTerminal,
  explorer: IconExternal,
};

/**
 * The workspace toolbar.
 *
 * Project identity now lives in the sidebar, where the things scoped to it live too, so
 * this row carries only what is true about the working state right now — the boundary,
 * the branch — and the actions that change it.
 */
export function ProjectHeader({
  project,
  tools,
  workbenchOpen,
  workbenchMode,
  onToggleWorkbench,
  onOpenRules,
  onOpenReview,
  onProjectChange,
  organizeAction,
}: {
  project: ProjectSummary;
  tools: ToolAvailability[];
  workbenchOpen: boolean;
  workbenchMode: WorkbenchMode;
  onToggleWorkbench: () => void;
  onOpenRules: () => void;
  onOpenReview: () => void;
  onProjectChange: (project: ProjectSummary) => void;
  organizeAction?: ReactNode;
}) {
  const [open, setOpen] = useState(false);
  const [currentTools, setCurrentTools] = useState<ToolAvailability[]>(tools);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (tools.length > 0) {
      setCurrentTools(tools);
    }
  }, [tools]);

  useEffect(() => {
    if (!open) return;
    void api.projectTools().then(setCurrentTools).catch(() => {});
    const escape = (event: KeyboardEvent) => event.key === "Escape" && setOpen(false);
    window.addEventListener("keydown", escape);
    return () => window.removeEventListener("keydown", escape);
  }, [open]);

  const describe = (thrown: unknown) => {
    const value = thrown as { message?: string; hint?: string };
    setError([value.message, value.hint].filter(Boolean).join(" — "));
  };

  const launch = async (tool: ProjectTool) => {
    setError(null);
    try {
      await api.openProjectIn(project.path, tool);
      setOpen(false);
    } catch (launchError) {
      describe(launchError);
    }
  };

  const initializeGit = async () => {
    setError(null);
    try {
      onProjectChange(await api.initializeGit(project.path));
      setOpen(false);
    } catch (gitError) {
      describe(gitError);
    }
  };

  return (
    <header className="project-header">
      <div className="project-facts">
        <span className="workspace-lock" title="Bhippi sessions and owned actions are confined to this project folder.">
          <i aria-hidden="true" /> Workspace locked
        </span>
        <span className="project-path" title={project.path}>
          {clipPath(project.path, 44)}
        </span>
        {project.is_git_repository ? (
          <span title="Current Git branch">
            <IconGitBranch size={12} /> {project.branch ?? "repository"}
          </span>
        ) : (
          <span>not a Git repository</span>
        )}
      </div>

      <div className="project-actions">
        {organizeAction}

        <button className="review-btn" onClick={onOpenReview} title="Review changes made by AI in this workspace">
          <IconGitMerge size={13} /> Review
        </button>

        <button className="project-quiet" onClick={onOpenRules} title="Standing instructions for the agent here">
          <IconRules size={13} /> Rules
        </button>

        <button className="project-open" onClick={() => setOpen((value) => !value)} aria-expanded={open} aria-haspopup="menu">
          <IconExternal size={13} /> Open in <IconChevronDown size={11} />
        </button>

        {/* The workbench is closed on a fresh launch: an editor and a browser that open
            themselves would take two thirds of the window from the conversation the
            user actually came for. */}
        <button
          className={`workbench-toggle${workbenchOpen ? " on" : ""}`}
          onClick={onToggleWorkbench}
          aria-pressed={workbenchOpen}
          title={workbenchOpen ? "Hide the workbench" : "Show the editor and browser"}
        >
          <span className="workbench-toggle-glyph" aria-hidden="true">
            {workbenchMode === "browser" ? <IconBrowser size={13} /> : <IconEditor size={13} />}
          </span>
          <IconPanelRight size={13} />
        </button>

        {open ? (
          <>
            <button className="menu-scrim" onClick={() => setOpen(false)} aria-label="Close tool menu" />
            <div className="tool-menu" role="menu">
              {currentTools.map((tool) => {
                const Glyph = TOOL_ICONS[tool.tool];
                return (
                  <button
                    key={tool.tool}
                    role="menuitem"
                    title={tool.available ? tool.hint : `${tool.hint} Click to try anyway.`}
                    onClick={() => void launch(tool.tool)}
                    className={!tool.available ? " tool-unavailable" : ""}
                  >
                    <Glyph size={14} />
                    <span>
                      <strong>{tool.label}</strong>
                      <small>{tool.available ? tool.hint : "Not detected — click to try"}</small>
                    </span>
                  </button>
                );
              })}
              {!project.is_git_repository ? (
                <button role="menuitem" onClick={() => void initializeGit()}>
                  <IconGitBranch size={14} />
                  <span>
                    <strong>Initialize Git</strong>
                    <small>Create a repository in this folder</small>
                  </span>
                </button>
              ) : null}
              {error ? (
                <div className="tool-error" role="alert">
                  {error}
                </div>
              ) : null}
            </div>
          </>
        ) : null}
      </div>
    </header>
  );
}
