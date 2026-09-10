import type { WorkspaceSession } from "../lib/ipc";
import { teamWorkers } from "./teamBoard.ts";

interface TeamBoardProps {
  sessions: WorkspaceSession[];
  projectPath: string;
  activeId: string | null;
  onOpen: (id: string) => void;
}

function statusWord(status: string): string {
  if (status === "running") return "working";
  if (status === "paused") return "waiting";
  return status;
}

export function TeamBoard({ sessions, projectPath, activeId, onOpen }: TeamBoardProps) {
  const workers = teamWorkers(sessions, projectPath, null);
  if (workers.length === 0) return null;
  return (
    <div className="studio-team-board" role="status" aria-label="Team">
      {workers.map((worker) => {
        const provider = worker.provider_label || worker.provider || "agent";
        const doing = worker.last_line?.trim() || "just started";
        const active = worker.id === activeId;
        return (
          <button
            key={worker.id}
            type="button"
            className={`studio-team-chip${active ? " is-active" : ""}`}
            onClick={() => onOpen(worker.id)}
            title={doing}
          >
            <span className="studio-team-provider">{provider}</span>
            <span className="studio-team-doing">
              is {statusWord(worker.status)} — {worker.title}
              {doing ? `: ${doing}` : ""}
            </span>
          </button>
        );
      })}
    </div>
  );
}
