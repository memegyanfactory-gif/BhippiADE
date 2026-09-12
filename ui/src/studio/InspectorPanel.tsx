// The Inspector drawer: a rail, a findings list, and one finding read in full (ADR-0056 §11).
//
// It lives in the studio's bottom dock rather than over the viewport, because the viewport is
// a real Godot window and nothing may be painted over it (INV-090). The rail stays narrow,
// the list stays dense, severity appears only as a small mark beside a word — and the whole
// panel draws exactly what Rust computed. There is no score in this file, no count, no
// verdict: those arrive on the report.
//
// The one thing it *does* own is the fix flow's shape (§17). Pressing Fix fetches a preview
// and shows it; the Apply button beside that preview is the only thing in the UI that can
// change the project, and it sends back the token the preview came with.

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { api } from "../lib/api";
import type {
  Finding,
  FixPreview,
  InspectRequest,
  InspectorCard,
  InspectorId,
  InspectorScanResult,
  Severity,
} from "../lib/ipc";
import {
  NO_FILTER,
  SEVERITY_FILLED,
  SEVERITY_LABEL,
  SEVERITY_ORDER,
  SEVERITY_TOKEN,
  offers,
  railCount,
  railScore,
  notMeasuredHint,
  visibleFindings,
  type FindingFilter,
} from "./inspectorView";
import "../styles/inspector.css";

/** The four states every surface can be in (INV-075). */
type Loadable<T> =
  | { state: "idle" }
  | { state: "loading" }
  | { state: "ready"; data: T }
  | { state: "error"; message: string };

export interface InspectorPanelProps {
  projectPath?: string;
  /**
   * A scan the studio asked for — from the `◉ Inspect` menu or a slash command. Changing
   * the object runs it; `null` leaves whatever is on screen alone.
   */
  request: InspectRequest | null;
  /** Bumped by the caller to re-run the same request. */
  requestNonce?: number;
  /** Open a file in the workbench at a line. */
  onOpenFile?: (path: string, line?: number) => void;
  /** Open a scene in the Godot workspace. */
  onOpenScene?: (scene: string) => void;
  /** Hand a finding to the chat as a normal agent task (§18). */
  onSendToAgent?: (task: string) => void;
}

function errorText(cause: unknown): string {
  const value = cause as { message?: string; hint?: string } | undefined;
  if (value && typeof value.message === "string") {
    return value.hint ? `${value.message} ${value.hint}` : value.message;
  }
  return String(cause);
}

function SeverityMark({ severity }: { severity: Severity }) {
  return (
    <span
      className={`inspector-sev${SEVERITY_FILLED[severity] ? " filled" : ""}`}
      style={{ "--sev": SEVERITY_TOKEN[severity] } as React.CSSProperties}
      aria-hidden="true"
    />
  );
}

export function InspectorPanel({
  projectPath,
  request,
  requestNonce = 0,
  onOpenFile,
  onOpenScene,
  onSendToAgent,
}: InspectorPanelProps) {
  const [rail, setRail] = useState<InspectorCard[]>([]);
  const [scan, setScan] = useState<Loadable<InspectorScanResult>>({ state: "idle" });
  const [filter, setFilter] = useState<FindingFilter>(NO_FILTER);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [preview, setPreview] = useState<Loadable<FixPreview>>({ state: "idle" });
  const [applied, setApplied] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const lastRun = useRef<string>("");

  useEffect(() => {
    let cancelled = false;
    api
      .inspectorRail()
      .then((cards) => {
        if (!cancelled) setRail(cards);
      })
      .catch(() => {
        // The rail is static; a failure here means the app is in trouble elsewhere and the
        // drawer stays usable with an empty rail rather than blanking.
      });
    return () => {
      cancelled = true;
    };
  }, []);

  const run = useCallback(
    async (next: InspectRequest) => {
      if (!projectPath) return;
      setScan({ state: "loading" });
      setPreview({ state: "idle" });
      setApplied(null);
      try {
        const result = await api.inspectorScan(projectPath, next);
        setScan({ state: "ready", data: result });
        setSelectedId(result.report.findings[0]?.id ?? null);
      } catch (cause) {
        setScan({ state: "error", message: errorText(cause) });
      }
    },
    [projectPath],
  );

  // A request from the studio: run it once per (request, nonce).
  useEffect(() => {
    if (!request || !projectPath) return;
    const key = `${requestNonce}:${JSON.stringify(request)}`;
    if (lastRun.current === key) return;
    lastRun.current = key;
    void run(request);
  }, [request, requestNonce, projectPath, run]);

  const report = scan.state === "ready" ? scan.data.report : null;
  const findings = report?.findings ?? [];
  const shown = useMemo(() => visibleFindings(findings, filter), [findings, filter]);
  const selected = useMemo(
    () => shown.find((finding) => finding.id === selectedId) ?? shown[0] ?? null,
    [shown, selectedId],
  );

  const handleFix = useCallback(
    async (finding: Finding) => {
      if (!projectPath) return;
      setPreview({ state: "loading" });
      try {
        const card = await api.inspectorPreviewFix(projectPath, finding.id);
        setPreview({ state: "ready", data: card });
      } catch (cause) {
        setPreview({ state: "error", message: errorText(cause) });
      }
    },
    [projectPath],
  );

  const handleApply = useCallback(
    async (card: FixPreview) => {
      if (!projectPath || busy) return;
      setBusy(true);
      try {
        await api.inspectorApplyFix(projectPath, card.finding_id, card.token);
        setApplied(`Applied · ${card.summary}`);
        setPreview({ state: "idle" });
        if (request) await run(request);
      } catch (cause) {
        setPreview({ state: "error", message: errorText(cause) });
      } finally {
        setBusy(false);
      }
    },
    [projectPath, busy, request, run],
  );

  const handleIgnore = useCallback(
    async (finding: Finding) => {
      if (!projectPath) return;
      try {
        await api.inspectorSetIgnored(projectPath, finding.id, true);
        if (request) await run(request);
      } catch (cause) {
        setScan({ state: "error", message: errorText(cause) });
      }
    },
    [projectPath, request, run],
  );

  const handleSend = useCallback(
    async (finding: Finding) => {
      if (!projectPath || !onSendToAgent) return;
      try {
        const task = await api.inspectorAgentTask(projectPath, finding.id);
        onSendToAgent(task);
      } catch (cause) {
        setScan({ state: "error", message: errorText(cause) });
      }
    },
    [projectPath, onSendToAgent],
  );

  if (!projectPath) {
    return (
      <div className="studio-dock-empty">
        <p>Open a game to inspect it.</p>
      </div>
    );
  }

  return (
    <div className="inspector-panel">
      {/* Rail (§11): narrow, compact, and never wider than the list beside it. */}
      <nav className="inspector-rail" aria-label="Inspectors">
        <button
          type="button"
          className={`inspector-rail-row${filter.inspector === null ? " active" : ""}`}
          aria-pressed={filter.inspector === null}
          onClick={() => setFilter((current) => ({ ...current, inspector: null }))}
        >
          <span className="inspector-rail-label">Overview</span>
          <span className="inspector-rail-score">
            {report?.health.score === null || report === null ? "—" : report.health.score}
          </span>
        </button>
        {rail.map((card) => {
          const count = railCount(report?.health ?? null, card.id);
          const active = filter.inspector === card.id;
          return (
            <button
              key={card.id}
              type="button"
              className={`inspector-rail-row${active ? " active" : ""}`}
              aria-pressed={active}
              title={card.blurb}
              onClick={() =>
                setFilter((current) => ({ ...current, inspector: active ? null : card.id }))
              }
            >
              <span className="inspector-rail-label">{card.label}</span>
              <span className="inspector-rail-count">{count === null ? "" : count}</span>
              <span className="inspector-rail-score">
                {railScore(report?.health ?? null, card.id)}
              </span>
            </button>
          );
        })}
      </nav>

      <div className="inspector-body">
        <header className="inspector-summary">
          {scan.state === "loading" ? (
            <span className="inspector-activity" aria-live="polite">
              <span className="inspector-spinner" aria-hidden="true" />
              Reading the project…
            </span>
          ) : report ? (
            <>
              <span className="inspector-count">
                {report.health.counts.total}{" "}
                {report.health.counts.total === 1 ? "finding" : "findings"}
              </span>
              {SEVERITY_ORDER.map((severity) => {
                const total =
                  severity === "critical"
                    ? report.health.counts.critical
                    : severity === "high"
                      ? report.health.counts.high
                      : severity === "medium"
                        ? report.health.counts.medium
                        : severity === "low"
                          ? report.health.counts.low
                          : severity === "suggestion"
                            ? report.health.counts.suggestion
                            : report.health.counts.info;
                if (total === 0) return null;
                const active = filter.severity === severity;
                return (
                  <button
                    key={severity}
                    type="button"
                    className={`inspector-chip${active ? " active" : ""}`}
                    aria-pressed={active}
                    onClick={() =>
                      setFilter((current) => ({
                        ...current,
                        severity: active ? null : severity,
                      }))
                    }
                  >
                    <SeverityMark severity={severity} />
                    {SEVERITY_LABEL[severity]} {total}
                  </button>
                );
              })}
              {report.health.incomplete_line ? (
                <span className="inspector-incomplete">{report.health.incomplete_line}</span>
              ) : null}
              {scan.state === "ready" && !scan.data.changes.new_ids.length &&
              !scan.data.changes.resolved.length ? null : (
                <span className="inspector-changes">
                  {scan.state === "ready" ? changesLine(scan.data) : ""}
                </span>
              )}
            </>
          ) : (
            <span className="studio-dock-note">Nothing scanned yet.</span>
          )}
          <div className="inspector-summary-spacer" />
          {applied ? <span className="inspector-applied">{applied}</span> : null}
          <button
            type="button"
            className="studio-action-btn"
            disabled={scan.state === "loading"}
            onClick={() => void run(request ?? { scope: "project" })}
          >
            {scan.state === "ready" ? "Rescan" : "Scan"}
          </button>
        </header>

        {scan.state === "error" ? (
          <div className="studio-dock-error" role="alert">
            {scan.message}
          </div>
        ) : null}
        {scan.state === "ready" && scan.data.memory_unavailable ? (
          <p className="studio-dock-warn">
            The Inspector could not read or write this project&rsquo;s memory, so resolved and
            ignored findings will not survive a restart.
          </p>
        ) : null}
        {report?.truncated.map((note) => (
          <p key={note} className="studio-dock-note">
            {note}
          </p>
        ))}

        <div className="inspector-split">
          <ul className="inspector-list" role="listbox" aria-label="Findings">
            {shown.length === 0 && scan.state === "ready" ? (
              <li className="studio-dock-note">
                {findings.length === 0
                  ? "Nothing found. That is a real answer, not an empty screen."
                  : "No finding matches this filter."}
              </li>
            ) : null}
            {shown.map((finding) => (
              <li key={finding.id}>
                <button
                  type="button"
                  className={`inspector-row${selected?.id === finding.id ? " active" : ""}`}
                  role="option"
                  aria-selected={selected?.id === finding.id}
                  onClick={() => setSelectedId(finding.id)}
                >
                  <SeverityMark severity={finding.severity} />
                  <span className="inspector-row-main">
                    <span className="inspector-row-title">{finding.title}</span>
                    <span className="inspector-row-where">{finding.where_label}</span>
                  </span>
                  <span className="inspector-row-meta">
                    {SEVERITY_LABEL[finding.severity]}
                  </span>
                </button>
              </li>
            ))}
          </ul>

          <section className="inspector-detail" aria-label="Finding detail">
            {selected ? (
              <>
                <h4 className="inspector-detail-title">{selected.title}</h4>
                <p className="inspector-detail-where">{selected.where_label}</p>

                <dl className="inspector-facts">
                  <dt>Cause</dt>
                  <dd>{selected.cause}</dd>
                  <dt>Impact</dt>
                  <dd>{selected.impact}</dd>
                  <dt>Recommended fix</dt>
                  <dd>{selected.recommendation}</dd>
                  <dt>Severity</dt>
                  <dd>
                    <SeverityMark severity={selected.severity} />
                    {SEVERITY_LABEL[selected.severity]} · {selected.code}
                  </dd>
                  <dt>Confidence</dt>
                  <dd>{selected.confidence}%</dd>
                </dl>

                {selected.evidence.length > 0 ? (
                  <ul className="inspector-evidence">
                    {selected.evidence.map((item) => (
                      <li key={`${item.source}:${item.claim}`}>
                        <span className="inspector-evidence-claim">{item.claim}</span>
                        <span className="inspector-evidence-source">{item.source}</span>
                      </li>
                    ))}
                  </ul>
                ) : null}

                <div className="inspector-actions">
                  {offers(selected, "open") && selected.location.file ? (
                    <button
                      type="button"
                      className="studio-action-btn"
                      onClick={() =>
                        onOpenFile?.(
                          selected.location.file ?? "",
                          selected.location.line ?? undefined,
                        )
                      }
                    >
                      Open
                    </button>
                  ) : null}
                  {offers(selected, "open_scene") && selected.location.scene ? (
                    <button
                      type="button"
                      className="studio-action-btn"
                      onClick={() => onOpenScene?.(selected.location.scene ?? "")}
                    >
                      {offers(selected, "locate") ? "Locate" : "Open scene"}
                    </button>
                  ) : null}
                  {offers(selected, "send_to_agent") && onSendToAgent ? (
                    <button
                      type="button"
                      className="studio-action-btn"
                      onClick={() => void handleSend(selected)}
                    >
                      Send to agent
                    </button>
                  ) : null}
                  {offers(selected, "fix") ? (
                    <button
                      type="button"
                      className="studio-action-btn primary"
                      onClick={() => void handleFix(selected)}
                    >
                      Fix
                    </button>
                  ) : null}
                  <button
                    type="button"
                    className="studio-action-btn"
                    onClick={() => void handleIgnore(selected)}
                  >
                    Ignore
                  </button>
                </div>

                {/* The gate, drawn (§17). Nothing has been written at this point. */}
                {preview.state === "loading" ? (
                  <p className="studio-dock-note" aria-busy="true">
                    Preparing the change…
                  </p>
                ) : null}
                {preview.state === "error" ? (
                  <div className="studio-dock-error" role="alert">
                    {preview.message}
                  </div>
                ) : null}
                {preview.state === "ready" && preview.data.finding_id === selected.id ? (
                  <div className="inspector-fix" role="group" aria-label="Proposed fix">
                    <div className="inspector-fix-head">
                      <span>Proposed fix</span>
                      <span className="inspector-fix-risk">Risk: {preview.data.risk}</span>
                    </div>
                    <p className="inspector-fix-summary">{preview.data.summary}</p>
                    <ol className="inspector-fix-steps">
                      {preview.data.steps.map((step, index) => (
                        <li key={`${index}-${step}`}>{step}</li>
                      ))}
                    </ol>
                    <p className="inspector-fix-files">
                      {preview.data.files.length}{" "}
                      {preview.data.files.length === 1 ? "file" : "files"} affected ·{" "}
                      {preview.data.files.join(", ")}
                    </p>
                    <div className="inspector-actions">
                      <button
                        type="button"
                        className="studio-action-btn"
                        onClick={() => setPreview({ state: "idle" })}
                      >
                        Cancel
                      </button>
                      <button
                        type="button"
                        className="studio-action-btn primary"
                        disabled={busy}
                        onClick={() => void handleApply(preview.data)}
                      >
                        {busy ? "Applying…" : "Apply fix"}
                      </button>
                    </div>
                  </div>
                ) : null}
              </>
            ) : filter.inspector && report ? (
              <NotScanned health={report.health} inspector={filter.inspector} />
            ) : (
              <p className="studio-dock-note">Select a finding to read it in full.</p>
            )}
          </section>
        </div>
      </div>
    </div>
  );
}

/** The Performance panel's honest empty state (§3). */
function NotScanned({
  health,
  inspector,
}: {
  health: InspectorScanResult["report"]["health"];
  inspector: InspectorId;
}) {
  const hint = notMeasuredHint(health, inspector);
  if (hint) {
    return (
      <div className="inspector-notmeasured">
        <p>Not measured yet</p>
        <p className="studio-dock-note">{hint}</p>
      </div>
    );
  }
  return <p className="studio-dock-note">Nothing found by this inspector.</p>;
}

function changesLine(result: InspectorScanResult): string {
  const parts: string[] = [];
  if (result.changes.new_ids.length > 0) parts.push(`${result.changes.new_ids.length} new`);
  if (result.changes.returned_ids.length > 0) {
    parts.push(`${result.changes.returned_ids.length} returned`);
  }
  if (result.changes.resolved.length > 0) {
    parts.push(`${result.changes.resolved.length} resolved`);
  }
  return parts.join(" · ");
}
