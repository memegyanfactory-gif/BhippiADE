// Settings › Usage. Every figure on this screen arrives from the Rust ledger; the
// panel formats and lays out, and computes nothing (INV: no business logic in TS).
//
// The chart is a stacked area — the question this screen answers is "where did it go",
// which is composition over time, so the bands must add up to the total printed above
// them. Series colour comes from the validated palette in ADR-0011, assigned per
// provider by Rust, and every series is also direct-labelled with its logo and name so
// identity never rests on colour alone.

import { useCallback, useEffect, useMemo, useState } from "react";
import type { ProviderUsage, UsageDayPoint, UsageSummary, UsageWindow } from "../lib/ipc";
import { api } from "../lib/api";
import { ProviderLogo } from "../components/ProviderLogo";
import { gaugeColor } from "../components/UsageRing";
import { percent, shortDate, tokens, usd } from "../lib/format";

const WINDOWS: { id: UsageWindow; label: string }[] = [
  { id: "day", label: "Today" },
  { id: "week", label: "7 days" },
  { id: "month", label: "30 days" },
  { id: "quarter", label: "90 days" },
];

/** Which measure the hero number, the chart, and the share bars all read. */
type Metric = "cost" | "tokens";

/** How many series the chart draws before the tail folds into one "Other" band. */
const MAX_SERIES = 7;

type Failure = { message: string; hint: string | null };

export function UsagePanel() {
  const [window, setWindow] = useState<UsageWindow>("month");
  const [metric, setMetric] = useState<Metric>("tokens");
  const [breakdown, setBreakdown] = useState<"provider" | "day">("provider");
  const [summary, setSummary] = useState<UsageSummary | null>(null);
  const [failure, setFailure] = useState<Failure | null>(null);
  const [loading, setLoading] = useState(true);
  const [confirming, setConfirming] = useState(false);

  const load = useCallback(async (next: UsageWindow, refreshAccounts = false) => {
    setLoading(true);
    try {
      setSummary(await api.usage(next, refreshAccounts));
      setFailure(null);
    } catch (error) {
      setFailure(asFailure(error));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void load(window);
  }, [load, window]);

  if (failure) {
    return (
      <>
        <h2 className="settings-heading">Usage</h2>
        <p className="usage-failure" role="alert">
          {failure.message}
        </p>
        {failure.hint ? <p className="settings-note">{failure.hint}</p> : null}
        <button className="btn-primary" onClick={() => void load(window)}>
          Retry
        </button>
      </>
    );
  }

  if (!summary) {
    return (
      <>
        <h2 className="settings-heading">Usage</h2>
        <p className="settings-note">Reading the ledger…</p>
      </>
    );
  }

  const spent = summary.total_tokens > 0;
  // Only backends that actually spent something get a band; a flat line along the axis
  // for an idle provider is noise pretending to be data.
  const active = summary.providers.filter((row) => row.total_tokens > 0);
  const hero = metric === "cost" ? usd(summary.total_cost_usd) : tokens(summary.total_tokens);

  return (
    <>
      <div className="usage-head">
        <div className="usage-head-title">
          <h2 className="settings-heading">Usage</h2>
          <span className="usage-range">{summary.range_label}</span>
          {loading ? <span className="usage-refreshing">refreshing…</span> : null}
        </div>

        <div className="usage-controls">
          <button
            type="button"
            className="btn-secondary usage-account-refresh"
            disabled={loading}
            onClick={() => void load(window, true)}
          >
            Refresh accounts
          </button>
          <Segmented
            label="Measure"
            options={[
              { id: "cost", label: "Cost" },
              { id: "tokens", label: "Tokens" },
            ]}
            value={metric}
            onChange={setMetric}
          />
          <Segmented
            label="Usage window"
            options={WINDOWS.map((entry) => ({ id: entry.id, label: entry.label }))}
            value={window}
            onChange={setWindow}
          />
        </div>
      </div>

      <div className="usage-hero">
        <div className="usage-hero-figure">
          <span className="usage-hero-value">{hero}</span>
          <span className="usage-hero-note">
            {summary.total_turns} {summary.total_turns === 1 ? "turn" : "turns"} ·{" "}
            {metric === "cost" ? "list-price estimate" : `${tokens(summary.tokens_per_turn)} per turn`}
          </span>

          <ul className="usage-legend" aria-label="Providers in this window">
            {active.length === 0 ? (
              <li className="usage-legend-empty">Nothing spent in this window yet.</li>
            ) : (
              active.map((row) => <LegendRow key={row.id} row={row} metric={metric} />)
            )}
          </ul>
        </div>

        <section className="usage-chart-block" aria-label="Daily usage">
          <h3 className="usage-subheading">
            {metric === "cost" ? "Daily cost" : "Daily tokens"}
            <span className="usage-chart-span">{summary.days.length} days</span>
          </h3>
          <Chart days={summary.days} providers={active} metric={metric} />
        </section>
      </div>

      <h3 className="usage-subheading">Totals</h3>
      <div className="usage-tiles">
        <Tile label="Processed tokens" value={tokens(summary.total_tokens)} note={summary.window_label} />
        <Tile label="Input" value={tokens(summary.total_input_tokens)} note="sent to the model" />
        <Tile label="Output" value={tokens(summary.total_output_tokens)} note="generated" />
        <Tile label="Per turn" value={tokens(summary.tokens_per_turn)} note="mean, both directions" />
        <Tile
          label="Estimated cost"
          value={usd(summary.total_cost_usd)}
          note={
            summary.providers.every((row) => !row.metered || row.cost_is_exact)
              ? "list prices, metered providers only"
              : "list prices; ~ rows use a vendor default"
          }
        />
        <Tile
          label="Answering now"
          value={summary.active.label}
          note={summary.active.available ? "reachable" : "not reachable"}
          text
        />
        {summary.active.balance_usd !== null && (
          <Tile
            label="Balance"
            value={usd(summary.active.balance_usd)}
            note="current account balance"
          />
        )}
      </div>

      <div className="usage-breakdown-head">
        <h3 className="usage-subheading">Breakdown</h3>
        <Segmented
          label="Breakdown grouping"
          options={[
            { id: "provider", label: "Provider" },
            { id: "day", label: "Day" },
          ]}
          value={breakdown}
          onChange={setBreakdown}
        />
      </div>

      {breakdown === "provider" ? (
        <table className="table usage-table">
          <thead>
            <tr>
              <th scope="col">Provider</th>
              <th scope="col">Account</th>
              <th scope="col">Weekly plan</th>
              <th scope="col">Local guard</th>
              <th scope="col" className="num">
                Share
              </th>
              <th scope="col" className="num">
                Tokens
              </th>
              <th scope="col" className="num">
                Turns
              </th>
              <th scope="col" className="num">
                Cost
              </th>
              <th scope="col" className="num">
                Daily cap
              </th>
            </tr>
          </thead>
          <tbody>
            {summary.providers.map((row) => (
              <Row
                key={row.id}
                row={row}
                metric={metric}
                active={row.id === summary.active_provider_id}
                onSummary={setSummary}
                onFailure={setFailure}
              />
            ))}
          </tbody>
        </table>
      ) : spent ? (
        <DayTable days={summary.days} providers={summary.providers} />
      ) : (
        <p className="usage-empty">Nothing has been spent in this window yet.</p>
      )}

      <div className="usage-foot">
        <button
          className={`usage-danger${confirming ? " armed" : ""}`}
          onClick={() => {
            if (!confirming) {
              setConfirming(true);
              return;
            }
            setConfirming(false);
            void api
              .clearUsage(null)
              .then(setSummary)
              .catch((error: unknown) => setFailure(asFailure(error)));
          }}
          onBlur={() => setConfirming(false)}
        >
          {confirming ? "Clear history · confirm?" : "Clear history"}
        </button>
        <span className="settings-note">
          Costs are estimated from published list prices, not a bill — subscription CLIs and
          local models bill nothing per token and show a dash. Ninety days are kept; clearing
          removes the counters, never a conversation.
        </span>
      </div>
    </>
  );
}

/** One row of pill buttons behaving as a radio group. */
function Segmented<T extends string>({
  label,
  options,
  value,
  onChange,
}: {
  label: string;
  options: { id: T; label: string }[];
  value: T;
  onChange: (next: T) => void;
}) {
  return (
    <div className="usage-segmented" role="radiogroup" aria-label={label}>
      {options.map((option) => (
        <button
          key={option.id}
          role="radio"
          aria-checked={value === option.id}
          className={`usage-segment${value === option.id ? " active" : ""}`}
          onClick={() => onChange(option.id)}
        >
          {option.label}
        </button>
      ))}
    </div>
  );
}

function LegendRow({ row, metric }: { row: ProviderUsage; metric: Metric }) {
  const share = metric === "cost" ? row.share_of_cost : row.share_of_tokens;
  const value = metric === "cost" ? usd(row.cost_usd) : tokens(row.total_tokens);
  const balanceDisplay = row.balance_usd !== null ? `Balance: ${usd(row.balance_usd)}` : "";
  return (
    <li className="usage-legend-row">
      <span className="usage-swatch" style={seriesStyle(row.color_slot)} aria-hidden="true" />
      <ProviderLogo id={row.id} size={14} />
      <span className="usage-legend-name">{row.label}</span>
      <span className="usage-legend-bar" aria-hidden="true">
        <span
          className="usage-legend-fill"
          style={{ width: percent(share), ...seriesStyle(row.color_slot) }}
        />
      </span>
      <span className="usage-legend-share">{percent(share)}</span>
      <span className="usage-legend-value">
        {metric === "cost" && !row.metered ? <span title="Nothing is billed per token">—</span> : value}
        {balanceDisplay}
      </span>
    </li>
  );
}

// ── Chart ─────────────────────────────────────────────────────────────

const PLOT = { width: 620, height: 170, left: 46, right: 8, top: 10, bottom: 20 };

/** A stacked area over the window, with a crosshair and a per-day tooltip. */
function Chart({
  days,
  providers,
  metric,
}: {
  days: UsageDayPoint[];
  providers: ProviderUsage[];
  metric: Metric;
}) {
  const [hover, setHover] = useState<number | null>(null);

  // The bands, heaviest first, with everything past the palette folded into one "Other"
  // rather than inventing a ninth hue.
  const series = useMemo(() => {
    const head = providers.slice(0, MAX_SERIES);
    const tail = providers.slice(MAX_SERIES);
    const bands = head.map((row) => ({
      id: row.id,
      label: row.label,
      slot: row.color_slot,
      ids: [row.id],
    }));
    if (tail.length > 0) {
      bands.push({
        id: "__other",
        label: `${tail.length} more`,
        slot: 7,
        ids: tail.map((row) => row.id),
      });
    }
    return bands;
  }, [providers]);

  const valueOf = useCallback(
    (day: UsageDayPoint, ids: string[]) =>
      day.providers
        .filter((entry) => ids.includes(entry.id))
        .reduce((sum, entry) => sum + (metric === "cost" ? entry.cost_usd : entry.total_tokens), 0),
    [metric],
  );

  const dayTotal = useCallback(
    (day: UsageDayPoint) => (metric === "cost" ? day.cost_usd : day.total_tokens),
    [metric],
  );

  const peak = days.reduce((high, day) => Math.max(high, dayTotal(day)), 0);
  const format = metric === "cost" ? usd : tokens;

  if (peak === 0) {
    return (
      <p className="usage-chart-empty">
        No {metric === "cost" ? "cost" : "tokens"} recorded in the last {days.length} days.
      </p>
    );
  }

  const plotWidth = PLOT.width - PLOT.left - PLOT.right;
  const plotHeight = PLOT.height - PLOT.top - PLOT.bottom;
  const step = days.length > 1 ? plotWidth / (days.length - 1) : 0;
  const x = (index: number) => PLOT.left + index * step;
  const y = (value: number) => PLOT.top + plotHeight - (value / peak) * plotHeight;

  // Cumulative tops, band by band, so each area sits on the one below it.
  const running = days.map(() => 0);
  const bands = series.map((band) => {
    const tops = days.map((day, index) => {
      running[index] += valueOf(day, band.ids);
      return running[index];
    });
    const upper = tops.map((value, index) => `${x(index)},${y(value)}`);
    const lower = tops
      .map((value, index) => ({ value: value - valueOf(days[index], band.ids), index }))
      .reverse()
      .map((point) => `${x(point.index)},${y(point.value)}`);
    return {
      ...band,
      line: `M${upper.join("L")}`,
      area: `M${upper.join("L")}L${lower.join("L")}Z`,
    };
  });

  const gridlines = [0, 0.5, 1];
  const marks = [0, Math.floor((days.length - 1) / 2), days.length - 1];
  const hovered = hover === null ? null : days[hover];

  return (
    <div className="usage-chart">
      <svg
        viewBox={`0 0 ${PLOT.width} ${PLOT.height}`}
        role="img"
        aria-label={`Daily ${metric === "cost" ? "cost" : "tokens"} over ${days.length} days, peaking at ${format(peak)}`}
        onMouseLeave={() => setHover(null)}
      >
        {gridlines.map((fraction) => (
          <g key={fraction}>
            <line
              className="usage-grid"
              x1={PLOT.left}
              x2={PLOT.width - PLOT.right}
              y1={y(peak * fraction)}
              y2={y(peak * fraction)}
            />
            <text
              className="usage-axis-label"
              textAnchor="end"
              x={PLOT.left - 8}
              y={y(peak * fraction) + 3.5}
            >
              {fraction === 0 ? "0" : format(peak * fraction)}
            </text>
          </g>
        ))}

        {/* Painted deepest band first so the 2px surface stroke separates each pair. */}
        {bands
          .slice()
          .reverse()
          .map((band) => (
            <path key={band.id} className="usage-area" d={band.area} style={seriesStyle(band.slot)} />
          ))}
        {bands
          .slice()
          .reverse()
          .map((band) => (
            <path key={band.id} className="usage-line" d={band.line} style={seriesStyle(band.slot)} />
          ))}

        {hovered ? (
          <line
            className="usage-crosshair"
            x1={x(hover ?? 0)}
            x2={x(hover ?? 0)}
            y1={PLOT.top}
            y2={PLOT.top + plotHeight}
          />
        ) : null}

        <line
          className="usage-baseline"
          x1={PLOT.left}
          x2={PLOT.width - PLOT.right}
          y1={PLOT.top + plotHeight}
          y2={PLOT.top + plotHeight}
        />

        {marks.map((index) => (
          <text
            key={index}
            className="usage-axis-label"
            x={x(index)}
            y={PLOT.height - 6}
            textAnchor={index === 0 ? "start" : index === days.length - 1 ? "end" : "middle"}
          >
            {shortDate(days[index]?.date ?? "")}
          </text>
        ))}

        {/* Hit targets are a full column wide so a one-pixel line is still hoverable. */}
        {days.map((day, index) => (
          <rect
            key={day.date}
            className="usage-hit"
            x={x(index) - step / 2}
            y={PLOT.top}
            width={Math.max(step, 2)}
            height={plotHeight}
            onMouseEnter={() => setHover(index)}
          />
        ))}
      </svg>

      {hovered ? (
        <div
          className="usage-tip"
          role="status"
          style={{ left: `${((x(hover ?? 0) - PLOT.left) / plotWidth) * 100}%` }}
        >
          <span className="usage-tip-date">{shortDate(hovered.date)}</span>
          <span className="usage-tip-total">{format(dayTotal(hovered))}</span>
          {hovered.providers.length === 0 ? (
            <span className="usage-tip-row">nothing spent</span>
          ) : (
            hovered.providers
              .slice()
              .sort((a, b) => b.total_tokens - a.total_tokens)
              .map((entry) => (
                <span key={entry.id} className="usage-tip-row">
                  <span
                    className="usage-swatch"
                    style={seriesStyle(slotFor(providers, entry.id))}
                    aria-hidden="true"
                  />
                  {labelFor(providers, entry.id)}
                  <span className="usage-tip-value">
                    {metric === "cost" ? usd(entry.cost_usd) : tokens(entry.total_tokens)}
                  </span>
                </span>
              ))
          )}
        </div>
      ) : null}
    </div>
  );
}

function DayTable({ days, providers }: { days: UsageDayPoint[]; providers: ProviderUsage[] }) {
  const spentDays = days.filter((day) => day.total_tokens > 0).reverse();
  if (spentDays.length === 0) {
    return <p className="usage-empty">No day in this window recorded any usage.</p>;
  }
  return (
    <table className="table usage-table">
      <thead>
        <tr>
          <th scope="col">Day</th>
          <th scope="col">Providers</th>
          <th scope="col" className="num">
            Tokens
          </th>
          <th scope="col" className="num">
            Cost
          </th>
        </tr>
      </thead>
      <tbody>
        {spentDays.map((day) => (
          <tr key={day.date}>
            <td>{shortDate(day.date)}</td>
            <td>
              <span className="usage-day-providers">
                {day.providers.map((entry) => (
                  <span key={entry.id} className="usage-day-chip">
                    <span
                      className="usage-swatch"
                      style={seriesStyle(slotFor(providers, entry.id))}
                      aria-hidden="true"
                    />
                    {labelFor(providers, entry.id)}
                  </span>
                ))}
              </span>
            </td>
            <td className="num">{tokens(day.total_tokens)}</td>
            <td className="num">{day.cost_usd > 0 ? usd(day.cost_usd) : <span>—</span>}</td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}

/** The palette slot Rust assigned this provider (ADR-0011). */
function seriesStyle(slot: number): { color: string } {
  return { color: `var(--series-${(slot % 8) + 1})` };
}

function slotFor(providers: ProviderUsage[], id: string): number {
  return providers.find((row) => row.id === id)?.color_slot ?? 0;
}

function labelFor(providers: ProviderUsage[], id: string): string {
  return providers.find((row) => row.id === id)?.label ?? id;
}

function Tile({
  label,
  value,
  note,
  text = false,
}: {
  label: string;
  value: string;
  note: string;
  text?: boolean;
}) {
  return (
    <div className="usage-tile">
      <span className="usage-tile-label">{label}</span>
      <span className={text ? "usage-tile-value text" : "usage-tile-value"}>{value}</span>
      <span className="usage-tile-note">{note}</span>
    </div>
  );
}

function Row({
  row,
  metric,
  active,
  onSummary,
  onFailure,
}: {
  row: ProviderUsage;
  metric: Metric;
  active: boolean;
  onSummary: (summary: UsageSummary) => void;
  onFailure: (failure: Failure) => void;
}) {
  const capped = row.limit_tokens !== null;
  const share = metric === "cost" ? row.share_of_cost : row.share_of_tokens;
  const weekly = row.account?.weekly ?? null;
  const weeklyFraction = weekly?.used_fraction ?? null;
  const weeklyPercent = weeklyFraction == null ? null : percent(weeklyFraction);
  const accountName = row.account?.account_name ?? (
    row.account?.status === "signed_out" ? "Signed out" : "Not reported"
  );
  const accountDetail = [
    row.account?.plan?.toUpperCase(),
    row.balance_usd !== null ? `${usd(row.balance_usd)} balance` : null,
  ].filter(Boolean).join(" · ");

  return (
    <tr>
      <td>
        <span className="usage-row-name">
          <span className="usage-swatch" style={seriesStyle(row.color_slot)} aria-hidden="true" />
          <ProviderLogo id={row.id} size={16} />
          {row.label}
          {active ? <span className="usage-row-chip">answering</span> : null}
        </span>
      </td>
      <td>
        <span className="usage-account-name">{accountName}</span>
        {accountDetail ? <span className="usage-row-split">{accountDetail}</span> : null}
      </td>
      <td>
        {weeklyFraction == null ? (
          <span className="usage-row-uncapped" title={row.account?.note ?? "No vendor account data"}>
            not reported
          </span>
        ) : (
          <span className="usage-row-gauge">
            <span className="usage-bar">
              <span
                className="usage-bar-fill"
                style={{ width: weeklyPercent ?? "0%", background: gaugeColor(weeklyFraction) }}
              />
            </span>
            <span className="usage-row-percent">{weeklyPercent}</span>
            {weekly?.resets_at ? (
              <span className="usage-row-split">resets {formatReset(weekly.resets_at)}</span>
            ) : null}
          </span>
        )}
      </td>
      <td>
        {capped ? (
          <span className="usage-row-gauge">
            <span className="usage-bar">
              <span
                className="usage-bar-fill"
                style={{ width: percent(row.fraction), background: gaugeColor(row.fraction) }}
              />
            </span>
            <span className="usage-row-percent">{percent(row.fraction)}</span>
          </span>
        ) : (
          <span className="usage-row-uncapped">no cap</span>
        )}
      </td>
      <td className="num">{percent(share)}</td>
      <td className="num">
        {tokens(row.total_tokens)}
        <span className="usage-row-split">
          {tokens(row.input_tokens)} in · {tokens(row.output_tokens)} out
        </span>
      </td>
      <td className="num">{row.turns}</td>
      <td className="num">
        {row.metered ? (
          <span
            className={row.cost_is_exact ? undefined : "usage-cost-approx"}
            title={
              row.cost_is_exact
                ? "Priced at each model's own published list rate."
                : "Part of this spend ran on a model with no published rate, so it is priced at the vendor's default-model rate."
            }
          >
            {row.cost_is_exact ? "" : "~"}
            {usd(row.cost_usd)}
          </span>
        ) : (
          <span title="Nothing is billed per token">—</span>
        )}
      </td>
      <td className="num">
        <CapField row={row} onSummary={onSummary} onFailure={onFailure} />
      </td>
    </tr>
  );
}

function formatReset(epochSeconds: number): string {
  return new Intl.DateTimeFormat(undefined, {
    weekday: "short",
    hour: "numeric",
    minute: "2-digit",
  }).format(new Date(epochSeconds * 1000));
}

// The cap is entered in thousands so the field stays four digits wide at 2M tokens.
function CapField({
  row,
  onSummary,
  onFailure,
}: {
  row: ProviderUsage;
  onSummary: (summary: UsageSummary) => void;
  onFailure: (failure: Failure) => void;
}) {
  const stored = row.limit_tokens === null ? "" : `${Math.round(row.limit_tokens / 1_000)}`;
  const [draft, setDraft] = useState(stored);
  const [lastStored, setLastStored] = useState(stored);

  // Adopt a value that changed underneath us (another row's edit refetches all rows).
  if (stored !== lastStored) {
    setLastStored(stored);
    setDraft(stored);
  }

  const commit = () => {
    if (draft === stored) return;
    const thousands = Number.parseInt(draft, 10);
    // An empty or zero field means "no ceiling"; the backend rejects a literal 0.
    const next = Number.isFinite(thousands) && thousands > 0 ? thousands * 1_000 : null;
    void api
      .setTokenCap(row.id, next)
      .then(onSummary)
      .catch((error: unknown) => onFailure(asFailure(error)));
  };

  return (
    <span className="usage-cap">
      <input
        className="usage-cap-input"
        inputMode="numeric"
        value={draft}
        placeholder="none"
        aria-label={`Daily cap for ${row.label}, in thousands of tokens`}
        onChange={(event) => setDraft(event.target.value.replace(/[^0-9]/g, ""))}
        onBlur={commit}
        onKeyDown={(event) => {
          if (event.key === "Enter") event.currentTarget.blur();
          if (event.key === "Escape") setDraft(stored);
        }}
      />
      <span className="usage-cap-unit">k</span>
    </span>
  );
}

function asFailure(error: unknown): Failure {
  if (typeof error === "object" && error !== null) {
    const value = error as { message?: unknown; hint?: unknown };
    if (typeof value.message === "string") {
      return {
        message: value.message,
        hint: typeof value.hint === "string" ? value.hint : null,
      };
    }
  }
  return { message: "The usage ledger could not be read.", hint: null };
}
