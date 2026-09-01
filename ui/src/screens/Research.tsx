import { useEffect, useState } from "react";
import type { TierBudgetView } from "../lib/ipc";
import { api } from "../lib/api";
import { TickerStrip } from "../chrome/TickerStrip";

const STAGES = [
  "plan",
  "expand",
  "synth",
  "facts",
  "write",
  "images",
  "seo",
  "review",
  "publish",
];

export function Research() {
  const [budgets, setBudgets] = useState<TierBudgetView[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [selected, setSelected] = useState("X6");
  const [topic, setTopic] = useState("");

  useEffect(() => {
    api
      .tierBudgets()
      .then((views) => {
        setBudgets(views);
        setSelected(views.find((view) => view.tier === "X6")?.tier ?? views[0]?.tier ?? "X6");
      })
      .catch((loadError) =>
        setError(String((loadError as Error).message ?? loadError)),
      );
  }, []);

  const budget = budgets?.find((view) => view.tier === selected) ?? null;

  return (
    <>
      {/* The ticker lives here since the chrome rework (ADR-0010) — it is a research
          feature, not furniture for every screen. Outside `.pane` so it never scrolls. */}
      <TickerStrip />
      <div className="pane">
        <div className="progress-rail active" hidden={budgets !== null || error === null} />
      <div className="pane-inner">
        <h1 className="screen-title">Research</h1>
        <p className="screen-sub">Start a run, watch it think, inspect its evidence.</p>

        <div className="research-input">
          <input
            value={topic}
            onChange={(event) => setTopic(event.target.value)}
            placeholder="What should Bhippi research?"
            aria-label="Research topic"
          />
          <button
            className="btn-primary"
            disabled
            title="The research engine lands in sprint S3 — the topic you type is kept."
          >
            Start ↵
          </button>
        </div>

        <div className="tier-row" role="radiogroup" aria-label="Depth">
          {(budgets ?? ["X2", "X6", "X12", "X24"].map((tier) => ({ tier }))).map(
            (view) => {
              const tier = typeof view === "string" ? view : view.tier;
              return (
                <button
                  key={tier}
                  role="radio"
                  aria-checked={selected === tier}
                  className={`tier-chip${selected === tier ? " active" : ""}`}
                  onClick={() => setSelected(tier)}
                >
                  {tier}
                </button>
              );
            },
          )}
        </div>

        {error ? (
          <div className="error-inline" role="alert">
            {error}
          </div>
        ) : null}

        {budget ? (
          <>
            <div className="budget-line">
              {budget.expansions} expansions · {budget.sources_min}–{budget.sources_max} sources ·{" "}
              ~{budget.wall_minutes} min · {budget.words_min}–{budget.words_max} words
            </div>

            <section className="settings-section">
              <h2 className="settings-heading">Stage rail</h2>
              <div className="stage-rail">
                {STAGES.map((stage) => (
                  <span key={stage} className="stage" style={{ display: "inline-flex" }}>
                    <span className="tick" />
                    {stage}
                    {stage !== STAGES[STAGES.length - 1] ? (
                      <span className="stage-sep" aria-hidden="true" />
                    ) : null}
                  </span>
                ))}
              </div>
              <div className="counter-line">0 sources · 0 dots · 0 primary · 0 contradictions</div>
            </section>

            <div className="map-canvas">
              Type a topic, or pick a story from the ticker — the map grows here.
            </div>

            <section className="settings-section" style={{ marginTop: 32 }}>
              <h2 className="settings-heading">What each depth buys</h2>
              <table className="table">
                <thead>
                  <tr>
                    <th>Tier</th>
                    <th>Expansions</th>
                    <th>Sources</th>
                    <th>Dots</th>
                    <th>Wall</th>
                    <th>Words</th>
                  </tr>
                </thead>
                <tbody>
                  {(budgets ?? []).map((view) => (
                    <tr key={view.tier}>
                      <td className="num">{view.tier}</td>
                      <td className="num">{view.expansions}</td>
                      <td className="num">
                        {view.sources_min}–{view.sources_max}
                      </td>
                      <td className="num">{view.target_dots}</td>
                      <td className="num">{view.wall_minutes}m</td>
                      <td className="num">
                        {view.words_min}–{view.words_max}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </section>
          </>
        ) : null}
        </div>
      </div>
    </>
  );
}
