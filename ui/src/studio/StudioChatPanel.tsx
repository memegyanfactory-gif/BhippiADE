import { useCallback, useEffect, useRef, useState } from "react";
import { PlanCard } from "./PlanCard";

export interface SubAgent {
  id: string;
  name: string;
  role: string;
  provider: string;
  status: "idle" | "working" | "completed";
}

export interface ChatMessage {
  id: string;
  sender: "user" | "bhippi";
  time: string;
  text: string;
  checklist?: string[];
  planCard?: boolean;
  sceneCard?: {
    path: string;
    actionLabel: string;
  };
}

interface StudioChatPanelProps {
  onOpenScene?: (scenePath: string) => void;
  onSceneUpdated?: (action: string) => void;
  subagents: SubAgent[];
  onUpdateSubagents: (agents: SubAgent[]) => void;
  onClose?: () => void;
}

const DEFAULT_MESSAGES: ChatMessage[] = [
  {
    id: "msg-1",
    sender: "user",
    time: "10:24 AM",
    text: "Create a simple platformer scene with a jelly character, floating platforms, coins, and soft lighting.",
  },
  {
    id: "msg-2",
    sender: "bhippi",
    time: "10:24 AM",
    text: "Here's your platformer plan and synthesized scene! ✨",
    planCard: true,
    checklist: [
      "Level geometry created",
      "Player (Jelly) created",
      "Coins placed",
      "Lighting updated",
      "Particles added",
      "Camera configured",
    ],
    sceneCard: {
      path: "scene/main.tscn",
      actionLabel: "Open in Scene >",
    },
  },
  {
    id: "msg-3",
    sender: "user",
    time: "10:26 AM",
    text: "Add a moving platform and some background clouds.",
  },
  {
    id: "msg-4",
    sender: "bhippi",
    time: "10:26 AM",
    text: "Done!",
    checklist: ["Moving platform added", "Background clouds added"],
    sceneCard: {
      path: "scene/main.tscn",
      actionLabel: "Updated >",
    },
  },
];

const AVAILABLE_PROVIDERS = [
  "Anthropic Claude 3.7 Sonnet",
  "OpenAI GPT-4o",
  "Google Gemini 2.5 Pro",
  "DeepSeek V3",
  "Local / Ollama",
];

export function StudioChatPanel({
  onOpenScene,
  onSceneUpdated,
  subagents,
  onUpdateSubagents,
  onClose,
}: StudioChatPanelProps) {
  const [messages, setMessages] = useState<ChatMessage[]>(DEFAULT_MESSAGES);
  const [inputText, setInputText] = useState("");
  const [isSubagentPopoverOpen, setIsSubagentPopoverOpen] = useState(false);
  const [newAgentName, setNewAgentName] = useState("");
  const [newAgentRole, setNewAgentRole] = useState("GDScript Specialist");
  const [newAgentProvider, setNewAgentProvider] = useState(AVAILABLE_PROVIDERS[0]);
  const messagesEndRef = useRef<HTMLDivElement | null>(null);

  const scrollToBottom = () => {
    messagesEndRef.current?.scrollIntoView({ behavior: "smooth" });
  };

  useEffect(() => {
    scrollToBottom();
  }, [messages]);

  const handleSend = useCallback(() => {
    const trimmed = inputText.trim();
    if (!trimmed) return;

    const userMsg: ChatMessage = {
      id: `msg-${Date.now()}`,
      sender: "user",
      time: new Date().toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" }),
      text: trimmed,
    };

    setMessages((prev) => [...prev, userMsg]);
    setInputText("");

    // Check for natural language sub-agent commands
    const lower = trimmed.toLowerCase();
    if (
      lower.includes("sub agent") ||
      lower.includes("subagent") ||
      lower.includes("provider") ||
      lower.includes("sub-agent")
    ) {
      // User asking to configure or make sub agents
      setTimeout(() => {
        let detectedProvider = AVAILABLE_PROVIDERS[0];
        if (lower.includes("claude") || lower.includes("anthropic")) {
          detectedProvider = "Anthropic Claude 3.7 Sonnet";
        } else if (lower.includes("gpt") || lower.includes("openai")) {
          detectedProvider = "OpenAI GPT-4o";
        } else if (lower.includes("gemini") || lower.includes("google")) {
          detectedProvider = "Google Gemini 2.5 Pro";
        } else if (lower.includes("deepseek")) {
          detectedProvider = "DeepSeek V3";
        } else if (lower.includes("local") || lower.includes("ollama")) {
          detectedProvider = "Local / Ollama";
        }

        const newAgent: SubAgent = {
          id: `agent-${Date.now()}`,
          name: `Agent ${subagents.length + 1}`,
          role: lower.includes("script") ? "GDScript Specialist" : lower.includes("asset") ? "Asset Pipeline" : "World Builder",
          provider: detectedProvider,
          status: "working",
        };

        const updatedAgents = [...subagents, newAgent];
        onUpdateSubagents(updatedAgents);

        const replyMsg: ChatMessage = {
          id: `msg-${Date.now() + 1}`,
          sender: "bhippi",
          time: new Date().toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" }),
          text: `Created sub-agent: **${newAgent.name}** (${newAgent.role}) assigned to **${detectedProvider}**! ✨`,
          checklist: [
            `Sub-agent squad updated (${updatedAgents.length} active)`,
            `Provider initialized: ${detectedProvider}`,
            `Task pipeline synchronized`,
          ],
        };
        setMessages((prev) => [...prev, replyMsg]);
      }, 500);
      return;
    }

    // Default conversational game-building response
    setTimeout(() => {
      const replyMsg: ChatMessage = {
        id: `msg-${Date.now() + 1}`,
        sender: "bhippi",
        time: new Date().toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" }),
        text: "Applied your changes directly to the scene!",
        checklist: [
          `Scene graph updated with: "${trimmed.slice(0, 32)}..."`,
          "Physics colliders recomputed",
          "Godot background sync complete",
        ],
        sceneCard: {
          path: "scene/main.tscn",
          actionLabel: "Updated >",
        },
      };
      setMessages((prev) => [...prev, replyMsg]);
      onSceneUpdated?.(trimmed);
    }, 600);
  }, [inputText, subagents, onUpdateSubagents, onSceneUpdated]);

  const handleAddSubagentManual = () => {
    if (!newAgentName.trim()) return;
    const newAgent: SubAgent = {
      id: `agent-${Date.now()}`,
      name: newAgentName.trim(),
      role: newAgentRole,
      provider: newAgentProvider,
      status: "working",
    };
    onUpdateSubagents([...subagents, newAgent]);
    setNewAgentName("");
  };

  const handleRemoveSubagent = (id: string) => {
    if (subagents.length <= 1) return; // Keep at least one
    onUpdateSubagents(subagents.filter((a) => a.id !== id));
  };

  return (
    <aside className="studio-chat-panel" aria-label="Bhippi AI Assistant">
      {/* Header */}
      <header className="studio-chat-header">
        <div className="studio-chat-title">
          <span className="studio-chat-sparkle">
            <svg width="15" height="15" viewBox="0 0 24 24" fill="currentColor">
              <path d="M12 2L14.4 8.6L21 11L14.4 13.4L12 20L9.6 13.4L3 11L9.6 8.6L12 2Z" />
            </svg>
          </span>
          <span>Bhippi AI</span>
        </div>
        <button
          type="button"
          className="studio-chat-close"
          onClick={onClose}
          aria-label="Close chat"
          title="Close chat"
        >
          <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.2">
            <line x1="18" y1="6" x2="6" y2="18" />
            <line x1="6" y1="6" x2="18" y2="18" />
          </svg>
        </button>
      </header>

      {/* Message Feed */}
      <div className="studio-chat-messages">
        {messages.map((msg) => (
          <div key={msg.id} className="studio-message-group">
            {/* Avatar */}
            <div className={`studio-avatar ${msg.sender}`}>
              {msg.sender === "user" ? (
                "Y"
              ) : (
                <svg width="14" height="14" viewBox="0 0 24 24" fill="currentColor">
                  <path d="M12 2C6.48 2 2 6.48 2 12s4.48 10 10 10 10-4.48 10-10S17.52 2 12 2zm-1 14H9V8h2v8zm4 0h-2V8h2v8z" />
                </svg>
              )}
            </div>

            {/* Content Body */}
            <div className="studio-message-body">
              <div className="studio-message-meta">
                <span className="studio-message-author">
                  {msg.sender === "user" ? "You" : "Bhippi"}
                </span>
                <span className="studio-message-time">{msg.time}</span>
              </div>

              <div className="studio-message-text">{msg.text}</div>

              {/* Plan Card (GAD-020) */}
              {msg.planCard && (
                <div style={{ marginTop: "10px", marginBottom: "10px" }}>
                  <PlanCard
                    onApprove={(_id, askMeFirst, _answers) => {
                      onSceneUpdated?.(
                        askMeFirst
                          ? "Approved with step-by-step confirmation"
                          : "Approved & Build initiated",
                      );
                    }}
                  />
                </div>
              )}

              {/* Task Checklist if present */}
              {msg.checklist && msg.checklist.length > 0 && (
                <div className="studio-task-checklist">
                  {msg.checklist.map((item, idx) => (
                    <div key={idx} className="studio-task-item">
                      <span className="studio-task-check">
                        <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="3">
                          <polyline points="20 6 9 17 4 12" />
                        </svg>
                      </span>
                      <span>{item}</span>
                    </div>
                  ))}
                </div>
              )}

              {/* Scene Card */}
              {msg.sceneCard && (
                <div
                  className="studio-scene-card"
                  onClick={() => onOpenScene?.(msg.sceneCard?.path ?? "scene/main.tscn")}
                  title="Open scene in viewer"
                >
                  {/* Stylized Thumbnail */}
                  <div
                    className="studio-scene-card-thumb"
                    style={{
                      background: "linear-gradient(135deg, #2b3952 0%, #ff7700 100%)",
                      display: "flex",
                      alignItems: "center",
                      justifyContent: "center",
                    }}
                  >
                    <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="#fff" strokeWidth="1.8">
                      <rect x="2" y="2" width="20" height="20" rx="3" />
                      <path d="M12 7v10M7 12h10" />
                    </svg>
                  </div>

                  <div className="studio-scene-card-info">
                    <span className="studio-scene-card-title">{msg.sceneCard.path}</span>
                    <span className="studio-scene-card-action">{msg.sceneCard.actionLabel}</span>
                  </div>

                  <span className="studio-scene-card-chevron">
                    <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                      <polyline points="9 18 15 12 9 6" />
                    </svg>
                  </span>
                </div>
              )}
            </div>
          </div>
        ))}
        <div ref={messagesEndRef} />
      </div>

      {/* Input Composer & Sub-Agent Pill */}
      <footer className="studio-chat-footer">
        <div className="studio-chat-composer">
          <input
            type="text"
            className="studio-chat-input"
            value={inputText}
            placeholder="Ask Bhippi to build your game..."
            onChange={(e) => setInputText(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter" && !e.shiftKey) {
                e.preventDefault();
                handleSend();
              }
            }}
          />
          <button
            type="button"
            className="studio-chat-send-btn"
            disabled={!inputText.trim()}
            onClick={handleSend}
            aria-label="Send prompt"
          >
            <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.6">
              <line x1="12" y1="19" x2="12" y2="5" />
              <polyline points="5 12 12 5 19 12" />
            </svg>
          </button>
        </div>

        {/* Sub-Agent Status Pill */}
        <div style={{ position: "relative" }}>
          <div
            className="studio-agent-status-bar"
            onClick={() => setIsSubagentPopoverOpen((prev) => !prev)}
            title="Manage AI sub-agents and providers"
          >
            <span className="studio-agent-dot" />
            <span>
              {subagents.length} agent{subagents.length > 1 ? "s" : ""} working
            </span>
            <svg width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.2">
              <polyline points="6 9 12 15 18 9" />
            </svg>
          </div>

          {/* Interactive Sub-Agent Manager Popover */}
          {isSubagentPopoverOpen && (
            <div className="subagent-popover" role="dialog" aria-label="Sub-Agents Manager">
              <div className="subagent-popover-head">
                <span>Active Sub-Agents</span>
                <button
                  type="button"
                  className="studio-chat-close"
                  onClick={() => setIsSubagentPopoverOpen(false)}
                >
                  ✕
                </button>
              </div>

              <div className="subagent-list">
                {subagents.map((agent) => (
                  <div key={agent.id} className="subagent-item">
                    <div className="subagent-item-info">
                      <span className="subagent-name">{agent.name}</span>
                      <span className="subagent-role">{agent.role}</span>
                    </div>
                    <div style={{ display: "flex", alignItems: "center", gap: "6px" }}>
                      <span className="subagent-provider-tag">{agent.provider.split(" ")[0]}</span>
                      {subagents.length > 1 && (
                        <button
                          type="button"
                          style={{ color: "#ff4d4f", fontSize: "11px", padding: "2px" }}
                          onClick={() => handleRemoveSubagent(agent.id)}
                          title="Remove subagent"
                        >
                          ✕
                        </button>
                      )}
                    </div>
                  </div>
                ))}
              </div>

              {/* Add Sub-Agent Controls */}
              <div style={{ display: "flex", flexDirection: "column", gap: "6px", borderTop: "1px solid rgba(255,255,255,0.08)", paddingTop: "8px" }}>
                <div style={{ fontSize: "11px", fontWeight: 600, color: "var(--studio-text-muted)" }}>
                  + Add Sub-Agent
                </div>
                <input
                  type="text"
                  placeholder="Sub-agent name (e.g. Asset Generator)"
                  value={newAgentName}
                  onChange={(e) => setNewAgentName(e.target.value)}
                  style={{
                    background: "rgba(255,255,255,0.06)",
                    border: "1px solid rgba(255,255,255,0.1)",
                    borderRadius: "5px",
                    color: "#fff",
                    padding: "4px 8px",
                    fontSize: "11.5px",
                  }}
                />
                <select
                  value={newAgentRole}
                  onChange={(e) => setNewAgentRole(e.target.value)}
                  style={{
                    background: "#161922",
                    border: "1px solid rgba(255,255,255,0.1)",
                    borderRadius: "5px",
                    color: "#fff",
                    padding: "4px 8px",
                    fontSize: "11.5px",
                  }}
                >
                  <option value="GDScript Specialist">GDScript Specialist</option>
                  <option value="Scene & World Builder">Scene & World Builder</option>
                  <option value="Asset & Shader Artist">Asset & Shader Artist</option>
                  <option value="Playtest & QA Agent">Playtest & QA Agent</option>
                </select>
                <select
                  value={newAgentProvider}
                  onChange={(e) => setNewAgentProvider(e.target.value)}
                  style={{
                    background: "#161922",
                    border: "1px solid rgba(255,255,255,0.1)",
                    borderRadius: "5px",
                    color: "#fff",
                    padding: "4px 8px",
                    fontSize: "11.5px",
                  }}
                >
                  {AVAILABLE_PROVIDERS.map((p) => (
                    <option key={p} value={p}>
                      {p}
                    </option>
                  ))}
                </select>
                <button
                  type="button"
                  onClick={handleAddSubagentManual}
                  disabled={!newAgentName.trim()}
                  style={{
                    background: "var(--studio-accent)",
                    color: "#fff",
                    borderRadius: "5px",
                    padding: "5px",
                    fontSize: "11.5px",
                    fontWeight: 600,
                  }}
                >
                  Deploy Sub-Agent
                </button>
              </div>
            </div>
          )}
        </div>
      </footer>
    </aside>
  );
}
