import { useEffect, useMemo, useRef, useState } from "react";
import type { ComponentType } from "react";
import type { AgentPhase, PermissionRequest, ToolAction, ToolActivity } from "../lib/ipc";
import { PhaseIndicator, PhaseGlyph } from "../components/AgentPhase";
import { DiffView } from "../components/DiffView";
import { api } from "../lib/api";
import {
  IconCheck,
  IconChevronDown,
  IconClose,
  IconCode,
  IconExtractDots,
  IconFetchUrl,
  IconMaximize,
  IconMinimize,
  IconPlan,
  IconReadSource,
  IconSearch,
  IconCheckProviders,
  IconMonitor,
  IconBox,
  FileGlyph,
} from "../components/icons";

const TOOL_ICONS: Record<ToolAction, ComponentType<{ size?: number }>> = {
  plan: IconPlan,
  search_web: IconSearch,
  read_source: IconReadSource,
  write_file: IconCode,
  fetch_url: IconFetchUrl,
  extract_dots: IconExtractDots,
  check_providers: IconCheckProviders,
  control_computer: IconMonitor,
  edit_engine: IconBox,
};

import { getFileBadge, parseToolTarget } from "../lib/toolTarget";

export { getFileBadge, parseToolTarget };

/**
 * The phase a recorded step represents.
 *
 * The engine already names the phase on the live event, but a step re-read from a
 * reloaded conversation has only its title, so this recovers the phase from the same
 * verbs `parseToolTarget` produces. Both paths therefore animate identically.
 */
export function phaseOfTool(tool: ToolActivity): AgentPhase {
  if (tool.state === "failed") return "failed";
  const { verb } = parseToolTarget(tool.title, tool.detail);
  switch (verb) {
    case "Edited":
      return "editing";
    case "Ran":
      return "running";
    case "Planned":
      return "planning";
    case "Searched":
      return "searching";
    case "Fetched":
      return "fetching";
    case "Wrote":
      return "writing";
    case "Tested":
      return "testing";
    case "Read":
      return "reading";
    default:
      return tool.state === "ok" ? "done" : "analyzing";
  }
}

function elapsed(since: number): string {
  const seconds = Math.max(0, Math.floor((Date.now() - since) / 1000));
  if (seconds < 60) return `${seconds}s`;
  return `${Math.floor(seconds / 60)}m ${seconds % 60}s`;
}

/**
 * Full-width activity bar docked above the composer / bottom of screen.
 *
 * Collapsed: a live coding animation bar with glowing syntax pulse, current task, and "See the work" trigger.
 * Expanded: drops up into a rich detailed timeline of pre-analysis, reasoning, file edits, and tools,
 * with a Fullscreen toggle for an immersive inspection view.
 */
export function ActivityDock({
  tools,
  thinking,
  thinkingElapsedMs,
  phase,
  streaming,
  permission,
  answered,
  onAllow,
  onDeny,
}: {
  tools: ToolActivity[];
  thinking?: string | null;
  thinkingElapsedMs?: number | null;
  phase: { label: string; since: number; kind: AgentPhase } | null;
  streaming: boolean;
  permission: PermissionRequest | null;
  answered: boolean;
  onAllow: () => void;
  onDeny: () => void;
}) {
  const [open, setOpen] = useState(false);
  const [isFullscreen, setIsFullscreen] = useState(false);
  const [, tick] = useState(0);
  const [diffs, setDiffs] = useState<{ files: any[] } | null>(null);
  const seen = useRef<Map<string, number>>(new Map());

  const waiting = permission !== null && !answered;

  for (const tool of tools) {
    if (!seen.current.has(tool.id)) seen.current.set(tool.id, Date.now());
  }

  useEffect(() => {
    if (waiting) setOpen(true);
  }, [waiting]);

  useEffect(() => {
    if (!streaming && !waiting) return;
    const timer = window.setInterval(() => tick((value) => value + 1), 1000);
    return () => window.clearInterval(timer);
  }, [streaming, waiting]);

  useEffect(() => {
    void api.reviewChanges().then((r) => setDiffs(r)).catch(() => setDiffs(null));
  }, [open]);

  const running = useMemo(() => tools.filter((tool) => tool.state === "running"), [tools]);
  const finished = tools.length - running.length;

  useEffect(() => {
    const escape = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        if (isFullscreen) {
          setIsFullscreen(false);
        } else {
          setOpen(false);
        }
      }
    };
    window.addEventListener("keydown", escape);
    return () => window.removeEventListener("keydown", escape);
  }, [isFullscreen]);

  if (!streaming && !waiting && tools.length === 0) return null;

  const currentTool = running[running.length - 1] ?? tools[tools.length - 1];

  let headline = "Work completed";
  if (waiting) {
    headline = permission.action;
  } else if (running.length > 0 && currentTool) {
    headline = currentTool.title;
  } else if (streaming) {
    if (thinking && thinking.trim().length > 0) {
      headline = thinkingElapsedMs
        ? `Thinking (${Math.max(1, Math.round(thinkingElapsedMs / 1000))}s)...`
        : "Thinking through solution...";
    } else if (tools.length > 0 && currentTool) {
      headline = currentTool.title;
    } else if (phase?.label) {
      headline = phase.label;
    } else {
      headline = "Generating response...";
    }
  } else if (tools.length > 0) {
    headline = `Completed ${tools.length} step${tools.length === 1 ? "" : "s"}`;
  }

  // Which phase the strip animates. A pending permission outranks everything: it is
  // the only state where the agent is not working but the user must act.
  const livePhase: AgentPhase = waiting
    ? "awaiting_permission"
    : running.length > 0 && currentTool
      ? phaseOfTool(currentTool)
      : streaming
        ? (phase?.kind ?? (thinking?.trim() ? "reasoning" : "streaming"))
        : "done";

  // The tail of the work, newest last. Four fits the bar at the narrowest width the
  // window allows; more would truncate mid-filename, which is worse than fewer.
  const recent = tools.slice(-4);

  const summary = waiting
    ? "needs your answer"
    : running.length > 0
      ? `${running.length} active step${running.length === 1 ? "" : "s"}`
      : streaming
        ? (thinking && thinking.trim().length > 0 ? "reasoning" : "generating")
        : `${tools.length} step${tools.length === 1 ? "" : "s"}`;

  return (
    <div
      className={`activity-dock${open ? " open" : ""}${waiting ? " waiting" : ""}${
        isFullscreen ? " fullscreen" : ""
      }`}
    >
      {open ? (
        <>
          <div
            className="activity-scrim"
            onClick={() => {
              setOpen(false);
              setIsFullscreen(false);
            }}
          />
          <div
            className={`activity-panel${isFullscreen ? " panel-fullscreen" : ""}`}
            role="dialog"
            aria-label="Agent work and execution details"
          >
            <div className="activity-panel-head">
              <div className="activity-panel-head-left">
                <span className="activity-panel-icon">
                  <IconCode size={14} />
                </span>
                <span className="activity-panel-title">
                  {waiting ? "Action Approval Required" : "Agent Work & Detailed Timeline"}
                </span>
                {tools.length > 0 ? (
                  <span className="activity-count">
                    {finished}/{tools.length} completed
                  </span>
                ) : null}
              </div>

              <div className="activity-panel-head-actions">
                <button
                  type="button"
                  className="activity-head-btn"
                  onClick={() => setIsFullscreen((prev) => !prev)}
                  title={isFullscreen ? "Exit Fullscreen (Esc)" : "Fullscreen Mode"}
                  aria-label="Toggle fullscreen"
                >
                  {isFullscreen ? <IconMinimize size={12} /> : <IconMaximize size={12} />}
                  <span className="btn-label">{isFullscreen ? "Exit" : "Fullscreen"}</span>
                </button>
                <button
                  type="button"
                  className="activity-close"
                  onClick={() => {
                    setOpen(false);
                    setIsFullscreen(false);
                  }}
                  title="Close panel"
                  aria-label="Close"
                >
                  <IconClose size={12} />
                </button>
              </div>
            </div>

            <div className="activity-panel-body">
              {/* Pre-Analysis / Exploration Overview */}
              {tools.length > 0 ? (
                <div className="activity-section">
                  <div className="activity-section-header">
                    <span>1. File Exploration &amp; Analysis</span>
                    <span className="activity-section-badge">{tools.length} actions</span>
                  </div>
                  <div className="activity-files-grid">
                    {tools.map((tool) => {
                      const { verb, fileName, lineRange } = parseToolTarget(
                        tool.title,
                        tool.detail
                      );
                      const badge = getFileBadge(tool.detail || tool.title);
                      return (
                        <div key={tool.id} className="activity-file-card">
<span className="activity-file-badge">
                        <FileGlyph name={badge.name} size={15} />
                      </span>
                          <div className="activity-file-info">
                            <span className="activity-file-name">{fileName}</span>
                            <span className="activity-file-sub">
                              {verb} {lineRange ? `lines ${lineRange}` : "context"}
                            </span>
                          </div>
                          <span className={`activity-file-status ${tool.state}`}>
                            {tool.state === "ok" ? (
                              <IconCheck size={12} />
                            ) : tool.state === "running" ? (
                              <span className="activity-file-dot running" aria-hidden="true" />
                            ) : (
                              <span className="activity-file-dot" aria-hidden="true" />
                            )}
                          </span>
                        </div>
                      );
                    })}
                  </div>
                </div>
              ) : null}

              {/* Chain of Thought Reasoning */}
              {thinking ? (
                <div className="activity-section">
                  <div className="activity-section-header">
                    <span>2. Internal Reasoning &amp; Architecture Decisions</span>
                    {thinkingElapsedMs ? (
                      <span className="activity-section-badge">
                        {Math.round(thinkingElapsedMs / 1000)}s
                      </span>
                    ) : null}
                  </div>
                  <div className="activity-thinking-card">
                    <pre className="activity-thinking-pre">{thinking}</pre>
                  </div>
                </div>
              ) : null}

              {/* Chronological Action Steps */}
              <div className="activity-section">
                <div className="activity-section-header">
                  <span>3. Execution Log &amp; Tool Steps</span>
                </div>
                <div className="activity-rows">
                  {tools.length === 0 && !waiting ? (
                    <div className="activity-idle">
                      <span className="activity-breathe">
                        {phase?.label ?? "Analyzing workspace and planning changes..."}
                      </span>
                    </div>
                  ) : null}

                  {tools.map((tool) => {
                    const Glyph = TOOL_ICONS[tool.action];
                    const started = seen.current.get(tool.id);
                    const { verb, fileName, lineRange } = parseToolTarget(
                      tool.title,
                      tool.detail
                    );
                    const badge = getFileBadge(tool.detail || tool.title);
                    return (
                      <div key={tool.id} className={`activity-row ${tool.state}`}>
                        <span className="activity-glyph">
                          {tool.state === "ok" ? (
                            <IconCheck size={12} />
                          ) : (
                            <Glyph size={13} />
                          )}
                        </span>
                        <span className="activity-copy">
                          <span
                            className={`activity-step-title${
                              tool.state === "running" ? " activity-breathe" : ""
                            }`}
                          >
<strong className="activity-step-verb">{verb}</strong>{" "}
                            <span className="activity-step-target">
                              <span className="inline-badge">
                                <FileGlyph name={badge.name} size={10} />
                              </span>{" "}
                              {fileName} {lineRange ? <span className="inline-lines">{lineRange}</span> : null}
                            </span>
                          </span>
                          <span className="activity-step-detail">{tool.detail || tool.title}</span>
                        </span>
                        <span className="activity-timer">
                          {tool.state === "running" && started
                            ? elapsed(started)
                            : tool.state === "ok"
                              ? "done"
                              : ""}
                        </span>
                      </div>
                    );
                  })}

                   {streaming && phase && running.length === 0 && tools.length > 0 ? (
                     <div className="activity-row running">
                       <span className="activity-glyph">
                         <IconPlan size={13} />
                       </span>
                       <span className="activity-copy">
                         <span className="activity-step-title activity-breathe">{phase.label}</span>
                       </span>
                       <span className="activity-timer">{elapsed(phase.since)}</span>
                     </div>
                   ) : null}
                 </div>
               </div>

               {/* Code Diffs */}
               {diffs && diffs.files.length > 0 ? (
                 <div className="activity-section">
                   <div className="activity-section-header">
                     <span>Changes</span>
                     <span className="activity-section-badge">
                       {diffs.files.reduce((s, f) => s + f.additions, 0)} added ·{" "}
                       {diffs.files.reduce((s, f) => s + f.deletions, 0)} removed
                     </span>
                   </div>
                   <DiffView diffs={diffs.files} />
                 </div>
               ) : null}

               {waiting ? (
                <div className="activity-ask">
                  <div className="activity-ask-row">
                    <span className="activity-ask-label">{permission.action}</span>
                    <span className={`activity-risk ${permission.risk}`}>{permission.risk}</span>
                  </div>
                  <p className="activity-ask-scope">{permission.scope}</p>
                  <p className="activity-ask-detail">{permission.detail}</p>
                  <div className="activity-ask-btns">
                    <button className="activity-btn-allow" onClick={onAllow}>
                      Allow
                    </button>
                    <button className="activity-btn-deny" onClick={onDeny}>
                      Deny
                    </button>
                  </div>
                </div>
              ) : null}
            </div>
          </div>
        </>
      ) : null}

      {/* Live Coding Animation Strip above the composer */}
      <div className="activity-live-bar-wrap">
        <button
          type="button"
          className={`activity-live-trigger${streaming ? " is-streaming" : ""}`}
          onClick={() => setOpen((v) => !v)}
          aria-expanded={open}
          aria-haspopup="dialog"
        >
          <div className="activity-live-left">
            <PhaseIndicator
              phase={livePhase}
              label={headline}
              since={waiting ? null : (phase?.since ?? null)}
            />
          </div>

          <div className="activity-live-center">
            {/* The last few steps, so the bar shows what actually happened rather
                than a decorative code ticker that was always the same three words. */}
            {recent.length > 0 ? (
              <span className="activity-recent" aria-hidden="true">
                {recent.map((tool) => {
                  const { verb, fileName } = parseToolTarget(tool.title, tool.detail);
                  return (
                    <span key={tool.id} className={`recent-chip ${tool.state}`}>
                      <PhaseGlyph phase={phaseOfTool(tool)} size={10} />
                      <span className="recent-verb">{verb}</span>
                      <span className="recent-file">{fileName}</span>
                    </span>
                  );
                })}
              </span>
            ) : null}
          </div>

          <div className="activity-live-right">
            <span className="activity-see-work-pill">
              <IconCode size={12} />
              <span>See the work</span>
              <IconChevronDown size={11} className={`see-work-chev${open ? " open" : ""}`} />
            </span>
            <span className="activity-summary">{summary}</span>
          </div>
        </button>
      </div>
    </div>
  );
}
