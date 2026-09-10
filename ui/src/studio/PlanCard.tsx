import { useState } from "react";
import type { GamePlanView } from "./types";

interface PlanCardProps {
  plan?: GamePlanView;
  onApprove?: (planId: string, askMeFirst: boolean, answers: Record<string, string>) => void;
  onEdit?: (planId: string) => void;
  isLoading?: boolean;
  error?: string | null;
}

const DEFAULT_PLAN: GamePlanView = {
  id: "plan-demo-01",
  title: "Cozy Floating Platformer",
  genre: "3D Platformer / Exploration",
  perspective: "Third-Person Orbit",
  artStyle: "Stylized Low-Poly / Warm Sunset",
  mechanics: [
    "Fluid Jelly movement with squish & stretch",
    "Translating wooden moving platforms with directional indicators",
    "Collectible golden coins with particle sparkle feedback",
    "Hazard waterfall drop zones with gentle respawn",
  ],
  systems: [
    { name: "Character Controller (Jelly slime)", desc: "Physics kinematic body with squish bounce", done: true },
    { name: "World & Terrain (Floating islands)", desc: "Mossy rock cylinder meshes with vegetation", done: true },
    { name: "Platform Mechanics (Moving platform)", desc: "Looping translation with player kinematic ride", done: true },
    { name: "Collectibles & Scoring (Coins)", desc: "Rotational trigger areas with sound effects", done: false },
    { name: "Post-Processing & Lighting", desc: "Golden hour sunset directional light & atmospheric sky", done: true },
  ],
  openQuestions: [
    {
      id: "q1",
      question: "What jump height style do you prefer for the Jelly slime?",
      options: [
        "Cozy & Float-y (higher apex, gentle descent)",
        "Snappy & Arcade (fast reactive jumps)",
        "Physics Realistic (heavy gravity)",
      ],
      selected: "Cozy & Float-y (higher apex, gentle descent)",
    },
    {
      id: "q2",
      question: "Should the coins respawn after level reset?",
      options: ["Yes, persistent loop", "No, single run challenge"],
      selected: "Yes, persistent loop",
    },
  ],
  approved: false,
};

export function PlanCard({
  plan = DEFAULT_PLAN,
  onApprove,
  onEdit,
  isLoading = false,
  error = null,
}: PlanCardProps) {
  const [askMeFirst, setAskMeFirst] = useState(false);
  const [answers, setAnswers] = useState<Record<string, string>>(() => {
    const initial: Record<string, string> = {};
    plan.openQuestions.forEach((q) => {
      initial[q.id] = q.selected ?? q.options[0];
    });
    return initial;
  });
  const [approved, setApproved] = useState(plan.approved);

  const handleSelectOption = (questionId: string, option: string) => {
    setAnswers((prev) => ({ ...prev, [questionId]: option }));
  };

  const handleApprove = () => {
    setApproved(true);
    onApprove?.(plan.id, askMeFirst, answers);
  };

  if (isLoading) {
    return (
      <div className="studio-plan-card" style={{ padding: "20px", textAlign: "center" }}>
        <div className="studio-spinner" style={{ margin: "0 auto 10px" }} />
        <span style={{ fontSize: "12px", color: "var(--studio-text-muted)" }}>
          Synthesizing game plan from your prompt...
        </span>
      </div>
    );
  }

  if (error) {
    return (
      <div className="studio-plan-card" style={{ borderLeft: "3px solid #ff4d4f" }}>
        <div style={{ fontWeight: 600, color: "#ff4d4f", fontSize: "12px", marginBottom: "4px" }}>
          Failed to generate game plan
        </div>
        <div style={{ fontSize: "11px", color: "var(--studio-text-muted)" }}>{error}</div>
      </div>
    );
  }

  return (
    <div className="studio-plan-card" role="region" aria-label="Game Plan Card">
      {/* Header */}
      <div className="studio-plan-head">
        <div>
          <div style={{ fontSize: "10px", textTransform: "uppercase", letterSpacing: "0.08em", color: "var(--studio-accent)", fontWeight: 700 }}>
            Game Plan · {plan.id}
          </div>
          <div style={{ fontSize: "14px", fontWeight: 700, color: "#ffffff", marginTop: "2px" }}>
            {plan.title}
          </div>
        </div>
        <span className="studio-tag" style={{ background: "rgba(255, 119, 0, 0.15)", color: "var(--studio-accent)" }}>
          {plan.genre}
        </span>
      </div>

      {/* Meta Pills */}
      <div style={{ display: "flex", gap: "6px", flexWrap: "wrap", margin: "8px 0" }}>
        <span className="studio-tag">👁️ {plan.perspective}</span>
        <span className="studio-tag">🎨 {plan.artStyle}</span>
      </div>

      {/* Mechanics & Systems Checklist */}
      <div style={{ marginTop: "10px" }}>
        <div style={{ fontSize: "11px", fontWeight: 600, color: "var(--studio-text-faint)", textTransform: "uppercase", letterSpacing: "0.05em", marginBottom: "6px" }}>
          Planned Systems ({plan.systems.filter((s) => s.done).length}/{plan.systems.length} ready)
        </div>
        <div style={{ display: "flex", flexDirection: "column", gap: "5px" }}>
          {plan.systems.map((s) => (
            <div
              key={s.name}
              style={{
                display: "flex",
                alignItems: "flex-start",
                gap: "8px",
                fontSize: "11.5px",
                background: "rgba(255,255,255,0.03)",
                padding: "4px 8px",
                borderRadius: "4px",
              }}
            >
              <span style={{ color: s.done ? "var(--studio-green)" : "var(--studio-text-faint)" }}>
                {s.done ? "✓" : "○"}
              </span>
              <div>
                <span style={{ fontWeight: 600, color: s.done ? "#ffffff" : "var(--studio-text-muted)" }}>
                  {s.name}:
                </span>{" "}
                <span style={{ color: "var(--studio-text-muted)" }}>{s.desc}</span>
              </div>
            </div>
          ))}
        </div>
      </div>

      {/* Open Questions with Selectable Options */}
      {plan.openQuestions.length > 0 && (
        <div style={{ marginTop: "14px", borderTop: "1px solid rgba(255,255,255,0.08)", paddingTop: "10px" }}>
          <div style={{ fontSize: "11px", fontWeight: 600, color: "var(--studio-accent)", marginBottom: "8px" }}>
            ✦ Decisions Needed:
          </div>
          {plan.openQuestions.map((q) => (
            <div key={q.id} style={{ marginBottom: "10px" }}>
              <div style={{ fontSize: "11.5px", fontWeight: 550, color: "#ffffff", marginBottom: "5px" }}>
                {q.question}
              </div>
              <div style={{ display: "flex", flexDirection: "column", gap: "4px" }}>
                {q.options.map((opt) => {
                  const isChosen = answers[q.id] === opt;
                  return (
                    <label
                      key={opt}
                      style={{
                        display: "flex",
                        alignItems: "center",
                        gap: "6px",
                        fontSize: "11px",
                        color: isChosen ? "#ffffff" : "var(--studio-text-muted)",
                        background: isChosen ? "rgba(255, 119, 0, 0.12)" : "rgba(255,255,255,0.02)",
                        border: isChosen ? "1px solid var(--studio-accent)" : "1px solid rgba(255,255,255,0.06)",
                        padding: "4px 8px",
                        borderRadius: "4px",
                        cursor: "pointer",
                        transition: "all 0.15s ease",
                      }}
                    >
                      <input
                        type="radio"
                        name={q.id}
                        value={opt}
                        checked={isChosen}
                        onChange={() => handleSelectOption(q.id, opt)}
                        style={{ accentColor: "var(--studio-accent)" }}
                      />
                      <span>{opt}</span>
                    </label>
                  );
                })}
              </div>
            </div>
          ))}
        </div>
      )}

      {/* Actions Bar */}
      <div style={{ marginTop: "14px", borderTop: "1px solid rgba(255,255,255,0.08)", paddingTop: "10px" }}>
        <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between", marginBottom: "10px" }}>
          <label
            style={{
              display: "flex",
              alignItems: "center",
              gap: "6px",
              fontSize: "11px",
              color: "var(--studio-text-muted)",
              cursor: "pointer",
            }}
          >
            <input
              type="checkbox"
              checked={askMeFirst}
              onChange={(e) => setAskMeFirst(e.target.checked)}
              style={{ accentColor: "var(--studio-accent)" }}
            />
            <span>Ask me before each system</span>
          </label>
        </div>

        <div style={{ display: "flex", gap: "8px" }}>
          {!approved ? (
            <button
              type="button"
              className="studio-btn studio-btn-primary"
              onClick={handleApprove}
              style={{ flex: 1, padding: "7px 12px", fontSize: "12px", fontWeight: 600 }}
            >
              🚀 Approve &amp; Build
            </button>
          ) : (
            <div
              style={{
                flex: 1,
                padding: "6px 12px",
                borderRadius: "6px",
                background: "rgba(50, 215, 75, 0.15)",
                border: "1px solid var(--studio-green)",
                color: "var(--studio-green)",
                fontSize: "12px",
                fontWeight: 600,
                textAlign: "center",
              }}
            >
              ✓ Plan Approved · Building...
            </div>
          )}

          <button
            type="button"
            className="studio-btn studio-btn-secondary"
            onClick={() => onEdit?.(plan.id)}
            style={{ padding: "7px 12px", fontSize: "12px" }}
          >
            ✏️ Edit
          </button>
        </div>
      </div>
    </div>
  );
}
