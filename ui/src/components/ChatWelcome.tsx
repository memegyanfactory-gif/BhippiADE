import logo from "../assets/logo.png";
import type { ProjectSummary } from "../lib/ipc";

export interface ChatWelcomeProps {
  project?: ProjectSummary | null;
  projects?: ProjectSummary[];
  onSelectProject?: (p: ProjectSummary) => void;
  onSelectPrompt: (prompt: string) => void;
}

/**
 * The empty conversation. The owner's call on 2026-09-09 supersedes GAD-016's starter pills
 * and the welcome title: the empty chat is the mark alone, small and nearly transparent.
 * The props stay so the caller's contract is unchanged if the space ever earns content again.
 */
export function ChatWelcome(_props: ChatWelcomeProps) {
  return (
    <div className="chat-welcome minimal">
      <img src={logo} className="chat-welcome-logo" alt="" draggable={false} />
    </div>
  );
}
