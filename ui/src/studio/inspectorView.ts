// What the Inspector drawer *draws* — and only that.
//
// Every number in the panel comes from Rust: the score, the per-inspector counts, the
// severity totals, the sentence that says the picture is incomplete, and the line printed
// under each finding's title. This file holds the presentation decisions a webview is
// allowed to make — which rows are visible given the filter chips, and what colour token a
// severity dot uses — and nothing else (R3).

import type { Finding, HealthReport, InspectorId, Severity } from "../lib/ipc";

/** Worst first, matching `bhippi_types::Severity`'s own order. */
export const SEVERITY_ORDER: Severity[] = [
  "critical",
  "high",
  "medium",
  "low",
  "suggestion",
  "info",
];

/** The word the chip shows. */
export const SEVERITY_LABEL: Record<Severity, string> = {
  critical: "Critical",
  high: "High",
  medium: "Medium",
  low: "Low",
  suggestion: "Suggestion",
  info: "Info",
};

/**
 * The CSS custom property each severity's small indicator uses. Severity is never the only
 * signal — every row also carries the word — so this stays within INV-034's "no colour-only
 * meaning", and the palette stays to the three the design system already defines (§33).
 */
export const SEVERITY_TOKEN: Record<Severity, string> = {
  critical: "var(--error)",
  high: "var(--error)",
  medium: "var(--warn)",
  low: "var(--warn)",
  suggestion: "var(--text-faint)",
  info: "var(--text-faint)",
};

/** How dark a severity's dot is, so Critical and High are not the same mark. */
export const SEVERITY_FILLED: Record<Severity, boolean> = {
  critical: true,
  high: false,
  medium: true,
  low: false,
  suggestion: false,
  info: false,
};

export interface FindingFilter {
  /** `null` shows every inspector. */
  inspector: InspectorId | null;
  /** `null` shows every severity. */
  severity: Severity | null;
  /** Free text over the title and the location line. */
  query: string;
}

export const NO_FILTER: FindingFilter = { inspector: null, severity: null, query: "" };

/** The rows the drawer shows, in the order Rust sorted them. */
export function visibleFindings(findings: Finding[], filter: FindingFilter): Finding[] {
  const needle = filter.query.trim().toLowerCase();
  return findings.filter((finding) => {
    if (filter.inspector && finding.inspector !== filter.inspector) return false;
    if (filter.severity && finding.severity !== filter.severity) return false;
    if (needle.length === 0) return true;
    return (
      finding.title.toLowerCase().includes(needle) ||
      finding.where_label.toLowerCase().includes(needle) ||
      finding.code.toLowerCase().includes(needle)
    );
  });
}

/**
 * The count beside one rail row — read from the health report Rust computed, never counted
 * here. A dimension the report does not carry has no number, which is different from zero.
 */
export function railCount(health: HealthReport | null, inspector: InspectorId): number | null {
  if (!health) return null;
  const dimension = health.dimensions.find((row) => row.inspector === inspector);
  if (!dimension) return null;
  return dimension.findings;
}

/** The score line for one rail row: a number, "—" for an inspector that did not answer. */
export function railScore(health: HealthReport | null, inspector: InspectorId): string {
  if (!health) return "—";
  const dimension = health.dimensions.find((row) => row.inspector === inspector);
  if (!dimension || dimension.score === null) return "—";
  return String(dimension.score);
}

/**
 * What the Performance panel says when there is no measurement: the coverage's own `how`,
 * which is the button the user should press. Never a number.
 */
export function notMeasuredHint(health: HealthReport | null, inspector: InspectorId): string | null {
  const dimension = health?.dimensions.find((row) => row.inspector === inspector);
  if (!dimension) return null;
  return dimension.coverage.state === "not_measured" ? dimension.coverage.how : null;
}

/** `true` when this finding's action list offers that action. */
export function offers(finding: Finding, action: string): boolean {
  return finding.actions.some((available) => available === action);
}
