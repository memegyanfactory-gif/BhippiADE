// The status-bar gauge and the panel that opens upward from it.
//
// It always reports on the provider that would answer right now, so the number under
// the ring and the model the composer is pointed at can never disagree.

import { useEffect, useRef, useState } from "react";
import type { ProviderUsage, UsageSummary } from "../lib/ipc";
import { ProviderLogo } from "../components/ProviderLogo";
import { UsageRing, gaugeColor } from "../components/UsageRing";
import { countdown, percent, tokens, usd } from "../lib/format";

type UsageMeterProps = {
  summary: UsageSummary | null;
  onManage: () => void;
};

export function UsageMeter({ summary, onManage }: UsageMeterProps) {
  const [open, setOpen] = useState(false);
  const wrap = useRef<HTMLDivElement>(null);

  // Click-away and Escape both close it; neither steals focus from the composer.
  useEffect(() => {
    if (!open) return undefined;
    const onPointer = (event: MouseEvent) => {
      if (!wrap.current?.contains(event.target as Node)) setOpen(false);
    };
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") setOpen(false);
    };
    window.addEventListener("mousedown", onPointer);
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("mousedown", onPointer);
      window.removeEventListener("keydown", onKey);
    };
  }, [open]);

  if (!summary) {
    return (
      <span className="usage-meter-idle" title="Reading the ledger">
        <UsageRing fraction={0} capped={false} />
        usage
      </span>
    );
  }

  const active = summary.active;
  const capped = active.limit_tokens !== null;
  const reading = capped ? percent(active.fraction) : tokens(active.total_tokens);
  const others = summary.providers.filter(
    (row) => row.id !== active.id && row.total_tokens > 0,
  );

  return (
    <div className="usage-meter" ref={wrap}>
      <button
        className={`usage-trigger${open ? " open" : ""}`}
        onClick={() => setOpen((current) => !current)}
        aria-expanded={open}
        aria-haspopup="dialog"
        title={`${active.label} — ${tokens(active.total_tokens)} tokens ${summary.window_label.toLowerCase()}`}
      >
        <UsageRing fraction={active.fraction} capped={capped} />
        <span className="usage-reading">{reading}</span>
      </button>

      {open ? (
        <div className="usage-dropup" role="dialog" aria-label="Usage">
          <header className="usage-dropup-head">
            <ProviderLogo id={active.id} size={16} />
            <span className="usage-dropup-name">{active.label}</span>
            <span className="usage-dropup-chip">answering</span>
          </header>

          <Meter
            label={summary.window_label}
            used={active.total_tokens}
            limit={active.limit_tokens}
            fraction={active.fraction}
          />

          <dl className="usage-facts">
            <div>
              <dt>Turns</dt>
              <dd>{active.turns}</dd>
            </div>
            <div>
              <dt>Cost</dt>
              <dd title={active.metered ? "Estimated from list prices" : undefined}>
                {active.metered ? usd(active.cost_usd) : "not billed"}
              </dd>
            </div>
            <div>
              <dt>Resets</dt>
              <dd>in {countdown(summary.resets_in_seconds)}</dd>
            </div>
          </dl>

          {others.length > 0 ? (
            <section className="usage-others">
              <h3>Other providers</h3>
              {others.slice(0, 4).map((row) => (
                <OtherRow key={row.id} row={row} />
              ))}
            </section>
          ) : null}

          <footer className="usage-dropup-foot">
            <span>
              {tokens(summary.total_tokens)} across all providers · {usd(summary.total_cost_usd)}
            </span>
            <button
              onClick={() => {
                setOpen(false);
                onManage();
              }}
            >
              Manage
            </button>
          </footer>
        </div>
      ) : null}
    </div>
  );
}

function Meter({
  label,
  used,
  limit,
  fraction,
}: {
  label: string;
  used: number;
  limit: number | null;
  fraction: number;
}) {
  return (
    <div className="usage-meter-row">
      <div className="usage-meter-line">
        <span className="usage-meter-label">{label}</span>
        <span className="usage-meter-value">
          {limit === null ? `${tokens(used)} · no cap` : `${tokens(used)} / ${tokens(limit)}`}
        </span>
      </div>
      <div className="usage-bar" role="img" aria-label={`${percent(fraction)} of the cap spent`}>
        {limit === null ? null : (
          <span
            className="usage-bar-fill"
            style={{ width: percent(fraction), background: gaugeColor(fraction) }}
          />
        )}
      </div>
      <div className="usage-meter-line">
        <span className="usage-meter-sub">
          {limit === null ? "no ceiling set for this provider" : `${percent(fraction)} spent`}
        </span>
      </div>
    </div>
  );
}

function OtherRow({ row }: { row: ProviderUsage }) {
  const capped = row.limit_tokens !== null;
  return (
    <div className="usage-other">
      <ProviderLogo id={row.id} size={13} />
      <span className="usage-other-name">{row.label}</span>
      <span className="usage-other-bar" aria-hidden="true">
        {capped ? (
          <span
            style={{ width: percent(row.fraction), background: gaugeColor(row.fraction) }}
          />
        ) : null}
      </span>
      <span className="usage-other-value">{tokens(row.total_tokens)}</span>
    </div>
  );
}
