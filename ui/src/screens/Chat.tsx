import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type {
  AgentPhase,
  ChatTurnView,
  ConversationView,
  Effort,
  LimitSnapshot,
  PermissionRequest,
  PluginMetadata,
  ProviderInfo,
  ProjectSummary,
  DesignMode,
  Skill,
  ToolActivity,
  UsageSummary,
} from "../lib/ipc";
import { api, events } from "../lib/api";
import { clipName } from "../lib/format";
import { Markdown } from "../components/Markdown";
import { ActivityDock } from "./ActivityDock";
import { PhaseIndicator } from "../components/AgentPhase";
import { FaultCard } from "../components/FaultCard";
import { ChatUsageMeter } from "../components/ChatUsageMeter";
import { BhippiComputerPanel } from "../components/BhippiComputerPanel";
import { ChatWelcome } from "../components/ChatWelcome";
import {
  ActivityGroup,
  TurnChangesCard,
  TurnNotices,
  formatDuration,
  groupHeadline,
  groupTools,
} from "../components/TurnActivity";

import type { PermissionMode } from "../components/PermissionPicker";
import {
  ProviderPopover,
  ModelPopover,
  ThinkingPopover,
  PermissionPopover,
  OptionsPopover,
} from "../components/ComposerPopovers";
import { isVisionModel } from "../lib/vision";
import {
  IconArrowRight,
  IconArrowUp,
  IconBolt,
  IconMonitor,
  IconCheck,
  IconChevronDown,
  IconCopy,
  IconClose,
  IconEdit,
  IconFile,
  IconGitMerge,
  IconQueue,
  IconSplitView,
  IconBrowser,
  IconExternalLink,
  IconGear,
  IconMic,
  IconPlus,
  IconRefresh,
  IconSearch,
  IconShrink,
  IconSparkle,
  IconStop,
  IconTerminal,
  IconTrash,
  IconVision,
} from "../components/icons";
import { WebviewWindow } from "@tauri-apps/api/webviewWindow";
import type { SettingsTab } from "./SettingsModal";
import {
  getAudioSettings,
  onAudioSettingsChange,
  saveAudioSettings,
  transcribeAudioBlob,
  type AudioSettings,
} from "../lib/audio";


export type QueuedMessage = {
  id: string;
  text: string;
  providerId?: string | null;
  model?: string | null;
  effort?: Effort;
  createdAt: number;
};

const LOCALHOST_URL =
  /https?:\/\/(?:localhost|127\.0\.0\.1)(?::\d+)?(?:\/[^\s)\"'`<>]*)?/i;

const PREDICTIVE_STARTERS = [
  "Explain how this works",
  "Find the bug in this",
  "Refactor this for clarity",
  "Write tests for this",
  "Optimise this",
];

function firstLocalhostUrl(text: string): string | null {
  return text.match(LOCALHOST_URL)?.[0] ?? null;
}
const conversationDrafts = new Map<string, string>();

const conversationModels = new Map<string, Record<string, string>>();

// Each conversation owns its provider choice, so picking a provider in one chat never
// leaks into another (module scope, same lifetime as the per-conversation model maps).
const conversationProviders = new Map<string, string>();


const SLASH_COMMANDS = [
  {
    cmd: "/computer",
    label: "Use Computer",
    desc: "Force Computer Use for this prompt; add the desktop task after the command",
    icon: "computer",
  },
  {
    cmd: "/debug",
    label: "Debug Workspace",
    desc: "Deterministic compiler & type checks with zero LLM tokens (15s timeout)",
    icon: "debug",
  },
  {
    cmd: "/clear",
    label: "Clear Conversation",
    desc: "Reset this chat's memory and title so it feels brand new (works without AI)",
    icon: "clean",
  },
  {
    cmd: "/clean",
    label: "Clear Conversation (alias)",
    desc: "Alias of /clear: reset this chat's memory and title",
    icon: "clean",
  },
  {
    cmd: "/reset",
    label: "Reset Conversation",
    desc: "Hard reset: clear all chat memory and start fresh",
    icon: "clean",
  },
  {
    cmd: "/compact",
    label: "Compact Context",
    desc: "Condense earlier turns into a compact summary to save token budget",
    icon: "compact",
  },
  {
    cmd: "/context",
    label: "Token & Context Stats",
    desc: "Inspect session turn count, token overhead, and active project",
    icon: "help",
  },
  {
    cmd: "/tokens",
    label: "Token Budget (alias)",
    desc: "Alias of /context: show token and context overview",
    icon: "help",
  },
  {
    cmd: "/model",
    label: "Model Configuration",
    desc: "Show the active provider, model, and effort mode for this chat",
    icon: "skills",
  },
  {
    cmd: "/rules",
    label: "Workspace Rules",
    desc: "Preview active rules loaded from AGENTS.md or CLAUDE.md",
    icon: "debug",
  },
  {
    cmd: "/skills",
    label: "List AI Skills",
    desc: "List imported skills from Claude, Codex, Antigravity, and Cursor",
    icon: "skills",
  },
  {
    cmd: "/time",
    label: "Current Time",
    desc: "Show the local date and time from this machine",
    icon: "help",
  },
  {
    cmd: "/version",
    label: "App Version",
    desc: "Show the current Bhippi app version",
    icon: "help",
  },
  {
    cmd: "/help",
    label: "Commands & Skills Help",
    desc: "Show full reference for slash commands and @skill tagging",
    icon: "help",
  },
] as const;

function SlashCommandIcon({ kind }: { kind: (typeof SLASH_COMMANDS)[number]["icon"] }) {
  if (kind === "computer") return <IconVision size={15} />;
  if (kind === "debug") return <IconTerminal size={15} />;
  if (kind === "clean") return <IconTrash size={15} />;
  if (kind === "compact") return <IconShrink size={15} />;
  if (kind === "skills") return <IconSparkle size={15} />;
  return <IconSearch size={15} />;
}



function isTerminal(state: ChatTurnView["state"]): boolean {
  return state === "done" || state === "stopped" || state === "failed";
}

function isComputerPhaseLabel(label?: string | null): boolean {
  if (!label) return false;
  const lower = label.toLowerCase();
  return ["computer", "desktop", "screen", "mouse", "browser", "cursor", "screenshot"]
    .some((marker) => lower.includes(marker));
}

export function Chat({
  onRunningChange,
  chatOptions,
  defaultProviderId,
  lastModel,
  activeId,
  onOpenConversation,
  onConversationsChanged,
  project,
  projects,
  onSelectProject,
  onOpenReview,
  usage,
  onManageUsage,
  onOpenSettings,
  onNewConversation,
  onCloseConversation,
  onOpenBrowser,
  onRefreshUsage,
}: {
  onRunningChange: (label: string | null) => void;
  chatOptions: ProviderInfo[];
  defaultProviderId: string | null;
  /** Provider id → the model the user last picked for it, restored from config. */
  lastModel: Record<string, string>;
  /** Shared with the sidebar; `null` while the first load is in flight. */
  activeId: string | null;
  onOpenConversation: (id: string) => void;
  onConversationsChanged: () => void;
  project: ProjectSummary;
  projects?: ProjectSummary[];
  onSelectProject?: (p: ProjectSummary) => void;
  onOpenReview?: (turnTitle?: string | null) => void;
  usage?: UsageSummary | null;
  onManageUsage?: () => void;
  onOpenSettings?: (tab?: SettingsTab) => void;
  onNewConversation?: () => void;
  onCloseConversation?: () => void;
  onOpenBrowser?: (url?: string) => void;
  onRefreshUsage?: () => Promise<void> | void;
}) {
  const [view, setView] = useState<ConversationView | null>(null);
  const [input, setInput] = useState<string>(() => (activeId ? conversationDrafts.get(activeId) ?? "" : ""));
  const [sending, setSending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [phase, setPhase] = useState<{
    turnId: string;
    label: string;
    kind: AgentPhase;
    since: number;
  } | null>(null);
  // Where the account stands against the active backend's plan windows. Reported by
  // the vendor mid-turn, so it is fresh rather than a figure from the last rescan.
  const [limits, setLimits] = useState<{ provider: string; snapshot: LimitSnapshot; at: number } | null>(null);
  const [answered, setAnswered] = useState<Record<string, boolean>>({});
  const [copied, setCopied] = useState<string | null>(null);
  const [busyRemedy, setBusyRemedy] = useState<string | null>(null);
  /// CHT-115: which turns can still be put back. Asked of the backend rather than assumed,
  /// because the snapshot is session-scoped and budgeted — a turn from an hour ago may have
  /// been evicted, and offering a button that cannot work is worse than not offering it.
  const [undoableTurns, setUndoableTurns] = useState<Record<string, boolean>>({});
  const [undoingTurn, setUndoingTurn] = useState<string | null>(null);
  const [remedyProgress, setRemedyProgress] = useState<string | null>(null);
  const [usageOpen, setUsageOpen] = useState(false);
  const [providerOpen, setProviderOpen] = useState(false);
  const [thinkingOpen, setThinkingOpen] = useState(false);
  const [permissionOpen, setPermissionOpen] = useState(false);
  const [permissionMode, setPermissionMode] = useState<PermissionMode>(() => {
    try {
      const saved = localStorage.getItem("bhippi_permission_mode");
      if (saved === "auto" || saved === "full_access" || saved === "ask_approval") {
        return saved;
      }
    } catch {}
    return "ask_approval";
  });
  const [computerBrowser, setComputerBrowser] = useState<boolean>(() => {
    try {
      const saved = localStorage.getItem("bhippi_permission_computer_browser");
      return saved !== null ? saved === "true" : false;
    } catch {
      return false;
    }
  });

  const [focusMode, setFocusMode] = useState<boolean>(() => {
    try {
      return localStorage.getItem("bhippi_focus_mode") === "on";
    } catch {
      return false;
    }
  });

  const [agentMode, setAgentMode] = useState<boolean>(() => {
    try {
      return localStorage.getItem("bhippi_agent_mode") === "on";
    } catch {
      return false;
    }
  });

  const [predictiveText, setPredictiveText] = useState<boolean>(() => {
    try {
      return localStorage.getItem("bhippi_predictive_text") === "on";
    } catch {
      return false;
    }
  });

  const [fontSize, setFontSize] = useState<number>(() => {
    try {
      const saved = localStorage.getItem("bhippi_font_size");
      return saved ? parseInt(saved, 10) || 15 : 15;
    } catch {
      return 15;
    }
  });

  useEffect(() => {
    try {
      localStorage.setItem("bhippi_focus_mode", focusMode ? "on" : "off");
    } catch {}
    document.documentElement.classList.toggle("bhippi-focus", focusMode);
    return () => document.documentElement.classList.remove("bhippi-focus");
  }, [focusMode]);

  useEffect(() => {
    try {
      localStorage.setItem("bhippi_agent_mode", agentMode ? "on" : "off");
    } catch {}
  }, [agentMode]);

  useEffect(() => {
    if (agentMode && permissionMode === "ask_approval") {
      setPermissionMode("auto");
    }
    // Saved Agent mode must auto-approve tools on launch, not only after a later toggle.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => {
    try {
      localStorage.setItem("bhippi_predictive_text", predictiveText ? "on" : "off");
    } catch {}
  }, [predictiveText]);

  useEffect(() => {
    try {
      localStorage.setItem("bhippi_font_size", String(fontSize));
    } catch {}
    document.documentElement.style.setProperty("--composer-font-size", `${fontSize}px`);
  }, [fontSize]);

  const permissionModeRef = useRef<PermissionMode>(permissionMode);
  useEffect(() => {
    permissionModeRef.current = permissionMode;
    try {
      localStorage.setItem("bhippi_permission_mode", permissionMode);
    } catch {}
  }, [permissionMode]);

  useEffect(() => {
    try {
      localStorage.setItem("bhippi_permission_computer_browser", String(computerBrowser));
    } catch {}
  }, [computerBrowser]);

  // The checkbox is a live view of the real gate: it mirrors Settings › Computer Use
  // (config.computer_use.enabled) so checking it actually lets the backend engage.
  useEffect(() => {
    api
      .computerUseStatus()
      .then((status) => setComputerBrowser(status.enabled))
      .catch(() => undefined);
  }, []);

  const toggleComputerBrowser = useCallback((next: boolean) => {
    setComputerBrowser(next);
    void api.setComputerUseEnabled(next).catch(() => undefined);
    void api.setComputerUseFullAccess(next).catch(() => undefined);
  }, []);

  // Audio & Voice Input state
  const [audioSettings, setAudioSettings] = useState<AudioSettings>(getAudioSettings());
  const [isRecording, setIsRecording] = useState(false);
  const [isTranscribing, setIsTranscribing] = useState(false);
  const [recordingSeconds, setRecordingSeconds] = useState(0);
  const [audioPromptOpen, setAudioPromptOpen] = useState(false);
  const activeAudioConfig = audioSettings.providers[audioSettings.activeSttProvider];

  const mediaRecorderRef = useRef<MediaRecorder | null>(null);
  const mediaStreamRef = useRef<MediaStream | null>(null);
  const speechRecognitionRef = useRef<any>(null);
  const audioChunksRef = useRef<Blob[]>([]);
  const timerIntervalRef = useRef<any>(null);

  useEffect(() => {
    return onAudioSettingsChange(setAudioSettings);
  }, []);

  // Cleanup microphone resources on unmount
  useEffect(() => {
    return () => {
      if (timerIntervalRef.current) clearInterval(timerIntervalRef.current);
      if (mediaStreamRef.current) {
        mediaStreamRef.current.getTracks().forEach((t) => t.stop());
      }
      if (speechRecognitionRef.current) {
        try {
          speechRecognitionRef.current.stop();
        } catch {}
      }
    };
  }, []);

  const stopVoiceRecording = useCallback(() => {
    if (speechRecognitionRef.current) {
      try {
        speechRecognitionRef.current.stop();
      } catch {}
      speechRecognitionRef.current = null;
    }
    if (mediaRecorderRef.current && mediaRecorderRef.current.state !== "inactive") {
      try {
        mediaRecorderRef.current.stop();
      } catch {}
      mediaRecorderRef.current = null;
    }
    if (mediaStreamRef.current) {
      mediaStreamRef.current.getTracks().forEach((t) => t.stop());
      mediaStreamRef.current = null;
    }
    setIsRecording(false);
    if (timerIntervalRef.current) {
      clearInterval(timerIntervalRef.current);
      timerIntervalRef.current = null;
    }
  }, []);

  const startVoiceRecording = useCallback(async () => {
    const currentSettings = getAudioSettings();
    const activeProvider = currentSettings.activeSttProvider;
    const config = currentSettings.providers[activeProvider];

    if (activeProvider !== "web_speech" && (!config?.apiKey || !config.apiKey.trim())) {
      setAudioPromptOpen(true);
      return;
    }

    setAudioPromptOpen(false);

    if (activeProvider === "web_speech") {
      const SpeechRec =
        (window as any).SpeechRecognition || (window as any).webkitSpeechRecognition;
      if (!SpeechRec) {
        setAudioPromptOpen(true);
        return;
      }
      try {
        const recognition = new SpeechRec();
        recognition.continuous = true;
        recognition.interimResults = true;
        recognition.lang =
          currentSettings.language === "auto" ? "en-US" : currentSettings.language;
        let baseInput = input.trim();

        recognition.onresult = (event: any) => {
          let transcript = "";
          for (let i = event.resultIndex; i < event.results.length; i++) {
            transcript += event.results[i][0].transcript;
          }
          if (transcript.trim()) {
            setInput(baseInput ? `${baseInput} ${transcript.trim()}` : transcript.trim());
          }
        };

        recognition.onerror = (e: any) => {
          console.warn("Speech recognition error:", e);
          stopVoiceRecording();
        };

        recognition.onend = () => {
          setIsRecording(false);
          if (timerIntervalRef.current) clearInterval(timerIntervalRef.current);
        };

        recognition.start();
        speechRecognitionRef.current = recognition;
        setIsRecording(true);
        setRecordingSeconds(0);
        timerIntervalRef.current = setInterval(() => {
          setRecordingSeconds((s) => s + 1);
        }, 1000);
      } catch (err) {
        console.error("Failed to start speech recognition:", err);
      }
      return;
    }

    // MediaRecorder flow for Deepgram / OpenAI / Groq / AssemblyAI / Custom
    try {
      const stream = await navigator.mediaDevices.getUserMedia({ audio: true });
      mediaStreamRef.current = stream;
      audioChunksRef.current = [];

      const mimeType = MediaRecorder.isTypeSupported("audio/webm;codecs=opus")
        ? "audio/webm;codecs=opus"
        : MediaRecorder.isTypeSupported("audio/webm")
          ? "audio/webm"
          : "";

      const recorder = mimeType
        ? new MediaRecorder(stream, { mimeType })
        : new MediaRecorder(stream);

      recorder.ondataavailable = (e) => {
        if (e.data && e.data.size > 0) {
          audioChunksRef.current.push(e.data);
        }
      };

      recorder.onstop = async () => {
        setIsRecording(false);
        if (timerIntervalRef.current) clearInterval(timerIntervalRef.current);
        const tracks = stream.getTracks();
        tracks.forEach((t) => t.stop());

        const blob = new Blob(audioChunksRef.current, {
          type: recorder.mimeType || "audio/webm",
        });

        if (blob.size > 400) {
          setIsTranscribing(true);
          try {
            const transcribed = await transcribeAudioBlob(blob, currentSettings);
            if (transcribed.trim()) {
              setInput((prev) =>
                prev ? `${prev.trimEnd()} ${transcribed.trim()}` : transcribed.trim(),
              );
              composerRef.current?.focus();
            }
          } catch (transcribeError: any) {
            console.error("Transcription error:", transcribeError);
            setError(transcribeError?.message || "Failed to transcribe voice recording.");
          } finally {
            setIsTranscribing(false);
          }
        }
      };

      recorder.start(250);
      mediaRecorderRef.current = recorder;
      setIsRecording(true);
      setRecordingSeconds(0);
      timerIntervalRef.current = setInterval(() => {
        setRecordingSeconds((s) => s + 1);
      }, 1000);
    } catch (err: any) {
      console.error("Microphone access denied or failed:", err);
      setError("Microphone permission denied or audio device not found.");
    }
  }, [input, stopVoiceRecording]);

  const handleToggleMic = () => {
    if (isRecording) {
      stopVoiceRecording();
    } else {
      void startVoiceRecording();
    }
  };


  // Remembered across sessions like effort: someone who works on interfaces wants it on
  // by default, and someone who does not never wants to see it again.
  const [designOn, setDesignOn] = useState<boolean>(() => {
    try {
      return localStorage.getItem("bhippi_design_mode") === "on";
    } catch {
      return false;
    }
  });
  // IndexMap: when ON, auto-indexes conversation context to help AI find things fast
  // using fewer tokens (similar to FixMap local-first repo mapping).
  // Default ON as requested.
  const [indexMapOn, setIndexMapOn] = useState<boolean>(() => {
    try {
      return localStorage.getItem("bhippi_index_map") === "on";
    } catch {
      return true; // default on
    }
  });
  // Caveman Language Mode: when ON, slashes token usage by stripping conversational filler
  // and forcing telegraphic responses while keeping code/diffs 100% complete and valid.
  const [cavemanOn, setCavemanOn] = useState<boolean>(() => {
    try {
      return localStorage.getItem("bhippi_caveman_mode") === "on";
    } catch {
      return false;
    }
  });
  const [modelOpen, setModelOpen] = useState(false);
  const [addMenuOpen, setAddMenuOpen] = useState(false);
  // Per-provider, so switching backend and back returns to the user's own choice.
  const [models, setModels] = useState<Record<string, string>>(() =>
    activeId ? conversationModels.get(activeId) ?? {} : {},
  );
  const [skills, setSkills] = useState<Skill[]>([]);
  const [menuIndex, setMenuIndex] = useState(0);
  const [plugins, setPlugins] = useState<PluginMetadata[]>([]);

  // ── Load installed plugins ──
  useEffect(() => {
    void api.listPlugins().then(setPlugins).catch(() => setPlugins([]));
  }, []);
  const [queuedMessages, setQueuedMessages] = useState<QueuedMessage[]>([]);
  const [isQueueCollapsed, setIsQueueCollapsed] = useState(false);
  const [previewOffer, setPreviewOffer] = useState<{ url: string } | null>(null);
  const [copiedQueuedId, setCopiedQueuedId] = useState<string | null>(null);
  const [, forceTick] = useState(0);

  const threadRef = useRef<HTMLDivElement | null>(null);
  const composerRef = useRef<HTMLTextAreaElement | null>(null);
  const stickToBottom = useRef(true);
  const offeredPreviewUrl = useRef<string | null>(null);

  const handleSelectPrompt = useCallback((promptText: string) => {
    setInput(promptText);
    setTimeout(() => {
      if (composerRef.current) {
        composerRef.current.focus();
        composerRef.current.selectionStart = composerRef.current.value.length;
        composerRef.current.selectionEnd = composerRef.current.value.length;
      }
    }, 0);
  }, []);
  // Several Chat surfaces may be mounted in Multi mode. Engine events carry turn ids,
  // so each surface keeps an ownership set and ignores every other session's stream.
  const ownedTurnIds = useRef<Set<string>>(new Set());

  // ── Load available skills ──
  useEffect(() => {
    void api.listSkills().then(setSkills).catch(() => setSkills([]));
  }, []);

  // ── Conversation loading (the list itself lives in the app shell) ──

  useEffect(() => {
    if (!activeId) {
      setView(null);
      ownedTurnIds.current = new Set();
      return;
    }
    let stale = false;
    ownedTurnIds.current = new Set();
    setError(null);
    stickToBottom.current = true;
    api
      .conversation(activeId)
      .then((fresh) => {
        if (!stale) {
          setView(fresh);
          ownedTurnIds.current = new Set(fresh?.turns.map((turn) => turn.id) ?? []);
        }
      })
      .catch((loadError) => {
        if (!stale) {
          setView(null);
          setError(String((loadError as Error).message ?? loadError));
        }
      });
    return () => {
      stale = true;
    };
  }, [activeId]);

  // ── Engine event stream ─────────────────────────────────────────────

  const mutateTurn = useCallback(
    (turnId: string, mutate: (turn: ChatTurnView) => ChatTurnView) => {
      setView((current) => {
        if (!current) return current;
        return {
          ...current,
          turns: current.turns.map((turn) => (turn.id === turnId ? mutate(turn) : turn)),
        };
      });
    },
    [],
  );

  const ownsTurn = useCallback((turnId: string) => ownedTurnIds.current.has(turnId), []);

  const resolveOwnedTurn = useCallback(async (turnId: string) => {
    if (ownsTurn(turnId)) return true;
    if (!activeId) return false;
    try {
      const fresh = await api.conversation(activeId);
      if (!fresh?.turns.some((turn) => turn.id === turnId)) return false;
      ownedTurnIds.current = new Set(fresh.turns.map((turn) => turn.id));
      setView(fresh);
      return true;
    } catch {
      return false;
    }
  }, [activeId, ownsTurn]);

  useEffect(() => {
    const unlisteners = [
      events.chatThinking.listen(({ payload }) => {
        void resolveOwnedTurn(payload.turn_id).then((owned) => {
          if (!owned) return;
          setPhase((current) => ({
            turnId: payload.turn_id,
            label: payload.label,
            kind: payload.phase,
            // A phase that repeats itself keeps its original clock; restarting the
            // timer on every identical event would show a turn as permanently new.
            since:
              current?.turnId === payload.turn_id && current.kind === payload.phase
                ? current.since
                : Date.now(),
          }));
        });
      }),
      events.chatLimits.listen(({ payload }) =>
        setLimits({ provider: payload.provider, snapshot: payload.limits, at: Date.now() }),
      ),
      events.chatThoughtDelta.listen(({ payload }) =>
        mutateTurn(payload.turn_id, (turn) => ({
          ...turn,
          state: turn.state === "queued" ? "streaming" : turn.state,
          thinking: (turn.thinking ?? "") + payload.delta,
        })),
      ),
      events.chatDelta.listen(({ payload }) =>
        mutateTurn(payload.turn_id, (turn) => ({
          ...turn,
          state: turn.state === "queued" ? "streaming" : turn.state,
          content: turn.content + payload.delta,
        })),
      ),
      events.chatTool.listen(({ payload }) => {
        mutateTurn(payload.turn_id, (turn) => {
          const existing = turn.tools.findIndex((tool) => tool.id === payload.tool.id);
          const tools = [...turn.tools];
          if (existing >= 0) tools[existing] = payload.tool;
          else tools.push(payload.tool);
          return { ...turn, tools };
        });
      }),
      events.chatPermissionRequested.listen(({ payload }) => {
        void resolveOwnedTurn(payload.turn_id).then((owned) => {
          if (!owned) return;
          if (permissionModeRef.current === "auto" || permissionModeRef.current === "full_access") {
            setAnswered((current) => ({ ...current, [payload.request.id]: true }));
            void api.respondPermission(payload.request.id, true).catch((e) => {
              console.error("Auto permission response error:", e);
            });
          } else {
            mutateTurn(payload.turn_id, (turn) => ({
              ...turn,
              state: "awaiting_permission",
              permission: payload.request,
            }));
          }
        });
      }),
      events.chatTurnDone.listen(({ payload }) => {
        void resolveOwnedTurn(payload.turn_id).then((owned) => {
          if (!owned) return;
          mutateTurn(payload.turn_id, (turn) => {
            const localhost = firstLocalhostUrl(turn.content);
            if (localhost && offeredPreviewUrl.current !== localhost) {
              offeredPreviewUrl.current = localhost;
              setPreviewOffer({ url: localhost });
            }
            return {
              ...turn,
              state: payload.state,
              provider: turn.provider ?? "assistant",
              fault: payload.fault,
            };
          });
          // A typed fault renders as a card inside the turn it belongs to. The banner
          // above the composer is only for failures with no turn to attach to.
          if (payload.error && !payload.fault) setError(payload.error);
          setPhase((current) => (current?.turnId === payload.turn_id ? null : current));
          setSending(false);
        });
      }),
      events.providerInstallProgress.listen(({ payload }) => {
        setRemedyProgress(payload.message);
      }),
    ];
    return () => {
      for (const unlisten of unlisteners) void unlisten.then((off) => off());
    };
  }, [mutateTurn, resolveOwnedTurn]);

  useEffect(() => onRunningChange(phase ? phase.label : null), [phase, onRunningChange]);

  const turns = view?.turns ?? [];

  // Ask once per finished turn that changed something. Cheap, and it keeps the button's
  // enabled state honest as the undo budget evicts older turns (CHT-115).
  useEffect(() => {
    const wanted = turns.filter(
      (turn) =>
        isTerminal(turn.state) &&
        (turn.changes?.files.length ?? 0) > 0 &&
        undoableTurns[turn.id] === undefined,
    );
    if (wanted.length === 0) return;
    let cancelled = false;
    void Promise.all(
      wanted.map(async (turn) => [turn.id, await api.turnUndoable(turn.id).catch(() => false)] as const),
    ).then((results) => {
      if (cancelled) return;
      setUndoableTurns((current) => {
        const next = { ...current };
        for (const [id, ok] of results) next[id] = ok;
        return next;
      });
    });
    return () => {
      cancelled = true;
    };
  }, [turns, undoableTurns]);

  /// Put a turn's files back, then mark it as no longer undoable — the snapshot is consumed.
  const undoTurn = useCallback(async (turn: ChatTurnView) => {
    setUndoingTurn(turn.id);
    try {
      await api.undoTurn(turn.id);
      setUndoableTurns((current) => ({ ...current, [turn.id]: false }));
    } catch {
      // The command's own message is the diagnosis; re-asking keeps the button truthful.
      setUndoableTurns((current) => ({ ...current, [turn.id]: false }));
    } finally {
      setUndoingTurn(null);
    }
  }, []);
  const activeAssistant = useMemo(
    () => turns.find((turn) => turn.role === "assistant" && !isTerminal(turn.state)) ?? null,
    [turns],
  );

  const streaming = activeAssistant !== null || sending;

  // High-precision elapsed time ticker while streaming or phase is active.
  useEffect(() => {
    if (!phase && !streaming) return;
    const timer = window.setInterval(() => forceTick((tick) => tick + 1), 200);
    return () => window.clearInterval(timer);
  }, [phase, streaming]);

  // ── Autocomplete computations ──────────────────────────────────────
  const isSlashMatch = input.startsWith("/") && !input.includes(" ");
  const slashSuggestions = isSlashMatch
    ? SLASH_COMMANDS.filter((sc) => sc.cmd.toLowerCase().startsWith(input.toLowerCase()))
    : [];

  const lastAtIdx = input.lastIndexOf("@");
  const isAtMatch =
    lastAtIdx >= 0 &&
    (lastAtIdx === 0 || input[lastAtIdx - 1] === " ") &&
    !input.slice(lastAtIdx).includes(" ");
  const atQuery = isAtMatch ? input.slice(lastAtIdx + 1).toLowerCase() : "";
  const skillSuggestions = isAtMatch
    ? skills.filter(
        (s) =>
          s.id.toLowerCase().includes(atQuery) ||
          s.name.toLowerCase().includes(atQuery) ||
          s.tags.some((t) => t.toLowerCase().includes(atQuery)),
      )
    : [];

  const showSlashMenu = isSlashMatch && slashSuggestions.length > 0;
  const showSkillMenu = !showSlashMenu && isAtMatch && skillSuggestions.length > 0;

  const prediction = useMemo(() => {
    if (!predictiveText) return null;
    const typed = input;
    if (!typed || typed.startsWith("/") || typed.includes("@")) return null;
    const needle = typed.toLowerCase();
    const fromHistory = [...turns]
      .reverse()
      .find(
        (turn) =>
          turn.role === "user" &&
          turn.content.toLowerCase().startsWith(needle) &&
          turn.content.length > typed.length,
      );
    if (fromHistory) return fromHistory.content.slice(typed.length);
    const hit = PREDICTIVE_STARTERS.find(
      (starter) => starter.toLowerCase().startsWith(needle) && starter.length > typed.length,
    );
    return hit ? hit.slice(typed.length) : null;
  }, [predictiveText, input, turns]);

  const selectSlashCommand = (cmd: string) => {
    setInput(cmd === "/computer" ? `${cmd} ` : cmd);
    setMenuIndex(0);
    composerRef.current?.focus();
  };

  const selectSkill = (skill: Skill) => {
    const prefix = input.slice(0, lastAtIdx);
    setInput(`${prefix}@${skill.id} `);
    setMenuIndex(0);
    composerRef.current?.focus();
  };

  // Extract skills currently tagged in the input text
  const taggedSkills = useMemo(() => {
    return skills.filter((s) => {
      const tag = `@${s.id.toLowerCase()}`;
      return input.toLowerCase().includes(tag);
    });
  }, [skills, input]);

  // If the conversation already has past assistant turns with a known provider, restore it
  const conversationTurnProviderId = useMemo(() => {
    if (!view?.turns) return null;
    const last = [...view.turns].reverse().find((t) => t.role === "assistant" && t.provider);
    if (!last?.provider) return null;
    const match = chatOptions.find(
      (opt) =>
        opt.id.toLowerCase() === last.provider?.toLowerCase() ||
        opt.label.toLowerCase() === last.provider?.toLowerCase(),
    );
    return match?.id ?? null;
  }, [view?.turns, chatOptions]);

  // Each conversation owns its provider choice, so picking a provider in one chat never
  // leaks into another (module scope + localStorage per session).
  const [chosenProvider, setChosenProvider] = useState<string | null>(() => {
    if (!activeId) return null;
    try {
      const stored = localStorage.getItem(`bhippi_chat_provider:${activeId}`);
      if (stored) return stored;
    } catch {}
    return conversationProviders.get(activeId) ?? null;
  });


  // Lock initial provider into this session so subsequent changes elsewhere never affect it
  useEffect(() => {
    if (!activeId) return;
    if (!chosenProvider && !conversationProviders.has(activeId)) {
      const initial = conversationTurnProviderId ?? defaultProviderId;
      if (initial) {
        setChosenProvider(initial);
        conversationProviders.set(activeId, initial);
        try {
          localStorage.setItem(`bhippi_chat_provider:${activeId}`, initial);
        } catch {}
      }
    }
  }, [activeId, chosenProvider, conversationTurnProviderId, defaultProviderId]);

  // Per-chat provider wins, completely isolated from any other chat window.
  const effectiveProviderId =
    chosenProvider ??
    (activeId ? conversationProviders.get(activeId) ?? null : null) ??
    conversationTurnProviderId ??
    defaultProviderId;
  const currentOption =
    chatOptions.find((option) => option.id === effectiveProviderId) ?? chatOptions[0] ?? null;

  const providerId = currentOption?.id ?? null;
  const defaultModelForProvider = providerId
    ? (lastModel[providerId] ?? currentOption?.models[0] ?? null)
    : null;
  const currentModel = providerId ? (models[providerId] ?? defaultModelForProvider) : null;

  // Snapshot each provider's starting model into this conversation. Later model changes
  // in another mounted chat must not leak through the shared lastModel config fallback.
  useEffect(() => {
    if (!activeId || !providerId || !defaultModelForProvider || models[providerId]) return;
    setModels((current) => {
      if (current[providerId]) return current;
      const next = { ...current, [providerId]: defaultModelForProvider };
      conversationModels.set(activeId, next);
      try {
        localStorage.setItem(`bhippi_chat_models:${activeId}`, JSON.stringify(next));
      } catch {}
      return next;
    });
  }, [activeId, providerId, defaultModelForProvider, models]);

  useEffect(() => {
    if (!providerId || !usage) return;
    const row =
      usage.providers.find((item) => item.id.toLowerCase() === providerId.toLowerCase()) ??
      (usage.active.id.toLowerCase() === providerId.toLowerCase() ? usage.active : null);
    const account = row?.account;
    if (!account || (account.weekly == null && account.session == null)) return;
    const accountAt = Date.parse(account.refreshed_at);
    setLimits((previous) => {
      if (
        previous &&
        previous.provider.toLowerCase() === providerId.toLowerCase() &&
        Number.isFinite(accountAt) &&
        previous.at > accountAt
      ) {
        return previous;
      }
      return {
        provider: providerId,
        at: Number.isFinite(accountAt) ? accountAt : Date.now(),
        snapshot: {
          status: account.status === "live" ? "allowed" : account.status,
          session_used: account.session?.used_fraction ?? null,
          session_resets_at: account.session?.resets_at ?? null,
          weekly_used: account.weekly?.used_fraction ?? null,
          weekly_resets_at: account.weekly?.resets_at ?? null,
        },
      };
    });
  }, [usage, providerId]);

  const chooseModel = useCallback(
    (model: string | null) => {
      if (!providerId) return;
      setModels((current) => {
        const next = { ...current };
        if (model === null) delete next[providerId];
        else next[providerId] = model;
        if (activeId) {
          conversationModels.set(activeId, next);
          try {
            localStorage.setItem(`bhippi_chat_models:${activeId}`, JSON.stringify(next));
          } catch {}
        }
        return next;
      });
    },
    [providerId, activeId],
  );

  // Per-chat provider choice. Strictly local to this session ID. Never updates any other chat.
  const chooseProvider = useCallback(
    (id: string | null) => {
      setChosenProvider(id);
      if (activeId) {
        if (id) {
          conversationProviders.set(activeId, id);
          try {
            localStorage.setItem(`bhippi_chat_provider:${activeId}`, id);
          } catch {}
        } else {
          conversationProviders.delete(activeId);
          try {
            localStorage.removeItem(`bhippi_chat_provider:${activeId}`);
          } catch {}
        }
        conversationModels.delete(activeId);
        try {
          localStorage.removeItem(`bhippi_chat_models:${activeId}`);
        } catch {}
        conversationDrafts.delete(activeId);
      }
      setModels({});
      forceTick((t) => t + 1);
    },
    [activeId],
  );

  const [effort, setEffort] = useState<Effort>(() => {
    if (!activeId) return "balanced";
    try {
      const stored = localStorage.getItem(`bhippi_chat_effort:${activeId}`);
      if (stored === "fast" || stored === "balanced" || stored === "quality" || stored === "ultra") {
        return stored as Effort;
      }
    } catch {}
    return "balanced";
  });

  const chooseEffort = useCallback(
    (next: Effort) => {
      setEffort(next);
      if (activeId) {
        try {
          localStorage.setItem(`bhippi_chat_effort:${activeId}`, next);
        } catch {}
      }
    },
    [activeId],
  );
  const design: DesignMode = designOn ? "on" : "off";
  const hasVision = isVisionModel(currentModel, providerId);

  const isComputerIntent = useMemo(() => {
    const lower = input.toLowerCase().trimStart();
    return (
      lower === "/computer" ||
      lower.startsWith("/computer ") ||
      lower.includes("use computer") ||
      lower.includes("use compuet") ||
      lower.includes("computer use") ||
      lower.includes("use pc") ||
      lower.includes("access my pc") ||
      lower.includes("access my computer") ||
      lower.includes("screenshot") ||
      lower.includes("screen") ||
      lower.includes("click on") ||
      lower.includes("automate desktop") ||
      lower.includes("move mouse") ||
      lower.includes("control the computer") ||
      lower.includes("control my computer")
    );
  }, [input]);

  const toggleDesign = useCallback(() => {
    setDesignOn((current) => {
      const next = !current;
      try {
        localStorage.setItem("bhippi_design_mode", next ? "on" : "off");
      } catch {
        // A browser that refuses storage still gets the toggle for this session.
      }
      return next;
    });
  }, []);


  // ── Scrolling ───────────────────────────────────────────────────────

  const scrollToBottom = useCallback((smooth = true) => {
    const thread = threadRef.current;
    if (!thread) return;
    thread.scrollTo({ top: thread.scrollHeight, behavior: smooth ? "smooth" : "auto" });
  }, []);

  useEffect(() => {
    if (stickToBottom.current) scrollToBottom(false);
  }, [view, scrollToBottom]);

  const onScroll = () => {
    const thread = threadRef.current;
    if (!thread) return;
    stickToBottom.current =
      thread.scrollHeight - thread.scrollTop - thread.clientHeight < 80;
  };

  // ── Auto-dispatch queued messages when agent finishes ──────────────
  useEffect(() => {
    if (!activeAssistant && !sending && queuedMessages.length > 0) {
      const [nextMsg, ...rest] = queuedMessages;
      setQueuedMessages(rest);
      void sendText(nextMsg.text, nextMsg.providerId, nextMsg.model, nextMsg.effort);
    }
  }, [activeAssistant, sending, queuedMessages]);

  // ── Actions ─────────────────────────────────────────────────────────

  const sendText = async (
    customText?: string,
    customProviderId?: string | null,
    customModel?: string | null,
    customEffort?: Effort,
  ) => {
    const text = (customText ?? input).trim();
    if (!text) return;

    // A hard clear stays "feels new": also drop this chat's saved model snapshot + draft
    // on the client, so the next turn starts from a fresh default. The backend clears the
    // conversation memory in send_chat_message (works with no provider configured).
    if (text === "/clear" || text === "/clean" || text === "/reset") {
      if (activeId) {
        conversationModels.delete(activeId);
        conversationDrafts.delete(activeId);
        try {
          localStorage.removeItem(`bhippi_chat_models:${activeId}`);
        } catch {}
      }
      setModels({});
      setQueuedMessages([]);
    }

    // If an assistant turn is running or sending, queue user input instead
    if ((sending || activeAssistant) && !customText) {
      const newQueued: QueuedMessage = {
        id: `q-${Date.now()}-${Math.random().toString(36).slice(2, 6)}`,
        text,
        providerId: effectiveProviderId,
        model: currentModel,
        effort,
        createdAt: Date.now(),
      };
      setQueuedMessages((prev) => [...prev, newQueued]);
      setInput("");
      if (activeId) conversationDrafts.delete(activeId);
      return;
    }

    setSending(true);
    setError(null);
    setInput("");
    if (activeId) conversationDrafts.delete(activeId);
    stickToBottom.current = true;
    try {
      const pair = await api.sendMessage(
        activeId,
        text,
        customProviderId ?? effectiveProviderId,
        customModel ?? currentModel,
        customEffort ?? effort,
        design,
        cavemanOn,
      );
      ownedTurnIds.current.add(pair.user_turn_id);
      ownedTurnIds.current.add(pair.assistant_turn_id);
      onOpenConversation(pair.conversation_id);
      onConversationsChanged();
      const fresh = await api.conversation(pair.conversation_id);
      setView(fresh);

      // Deterministic / offline commands complete immediately (0 AI tokens).
      // Check if the turns are empty (after /clear) or all already terminal:
      const isDone =
        !fresh ||
        fresh.turns.length === 0 ||
        fresh.turns.every((turn) => isTerminal(turn.state));
      if (isDone) {
        setSending(false);
        setPhase(null);
      }

      // ── Auto-IndexMap: when enabled, create a lightweight index of the context
      // so the AI can find relevant information fast without re-exploring (saves tokens)
      if (indexMapOn) {
        void triggerIndexMapIndexing(pair.conversation_id, text);
      }
    } catch (sendError) {
      setError(String((sendError as Error).message ?? sendError));
      if (!customText) {
        setInput((current) => (current.trim() ? current : text));
      }
      setSending(false);
    }
  };

  const send = () => sendText();

  const sendNow = async (id: string) => {
    const target = queuedMessages.find((m) => m.id === id);
    if (!target) return;
    setQueuedMessages((prev) => prev.filter((m) => m.id !== id));
    if (activeAssistant) {
      await stop();
    }
    void sendText(target.text, target.providerId, target.model, target.effort);
  };

  const editQueued = (id: string) => {
    const target = queuedMessages.find((m) => m.id === id);
    if (!target) return;
    setQueuedMessages((prev) => prev.filter((m) => m.id !== id));
    setInput(target.text);
    composerRef.current?.focus();
  };

  const deleteQueued = (id: string) => {
    setQueuedMessages((prev) => prev.filter((m) => m.id !== id));
  };

  const copyQueued = async (id: string) => {
    const target = queuedMessages.find((m) => m.id === id);
    if (!target) return;
    try {
      await navigator.clipboard.writeText(target.text);
      setCopiedQueuedId(id);
      window.setTimeout(
        () => setCopiedQueuedId((current) => (current === id ? null : current)),
        1600,
      );
    } catch {
      setError("Clipboard unavailable.");
    }
  };

  const forkQueued = async (id: string) => {
    const target = queuedMessages.find((m) => m.id === id);
    if (!target) return;
    setQueuedMessages((prev) => prev.filter((m) => m.id !== id));
    try {
      const meta = await api.newConversation();
      onConversationsChanged();
      const pair = await api.sendMessage(
        meta.id,
        target.text,
        target.providerId ?? effectiveProviderId,
        target.model ?? currentModel,
        target.effort ?? effort,
        design,
      );
      onOpenConversation(pair.conversation_id);
      onConversationsChanged();
    } catch (forkError) {
      setQueuedMessages((prev) => [target, ...prev]);
      setError(String((forkError as Error).message ?? forkError));
    }
  };

  const openLocalInBhippi = (url: string) => {
    setPreviewOffer(null);
    onOpenBrowser?.(url);
  };

  const openLocalInChrome = (url: string) => {
    setPreviewOffer(null);
    void api.openExternalUrl(url).catch((openError: unknown) => {
      setError(String((openError as Error).message ?? openError));
    });
  };

  const resolveInstallId = (hint?: string | null) => {
    const candidates = [hint, providerId].filter((value): value is string => Boolean(value));
    for (const candidate of candidates) {
      const wanted = candidate.trim().toLowerCase();
      const match = chatOptions.find(
        (option) => option.id.toLowerCase() === wanted || option.label.toLowerCase() === wanted,
      );
      if (match) return match.id;
    }
    return providerId;
  };

  const stop = async () => {
    setSending(false);
    setPhase(null);
    if (activeAssistant) {
      const turnId = activeAssistant.id;
      mutateTurn(turnId, (turn) => ({ ...turn, state: "stopped" }));
      try {
        await api.stopTurn(turnId);
      } catch (stopError) {
        setError(String((stopError as Error).message ?? stopError));
      }
    }
  };

  const regenerate = async (options?: { force?: boolean }) => {
    if (!activeId) return;
    if (!options?.force && (activeAssistant || sending)) return;
    if (options?.force && activeAssistant) {
      await stop();
    }
    setSending(true);
    setError(null);
    stickToBottom.current = true;
    try {
      const pair = await api.regenerate(activeId, effectiveProviderId, currentModel, effort, design, cavemanOn);
      ownedTurnIds.current.add(pair.user_turn_id);
      ownedTurnIds.current.add(pair.assistant_turn_id);
      setView(await api.conversation(activeId));
    } catch (regenerateError) {
      setError(String((regenerateError as Error).message ?? regenerateError));
      setSending(false);
    }
  };

  /**
   * Performs the remedy a fault card offered.
   *
   * Each branch is the *actual* fix for its failure rather than a generic retry: a full
   * context is compacted before resending, because resending it unchanged is guaranteed
   * to fail again; a stale CLI is reinstalled; a spent weekly window opens the picker,
   * because no amount of retrying clears a limit that resets on a billing boundary.
   */
  const applyRemedy = async (remedy: string, providerHint?: string | null) => {
    setBusyRemedy(remedy);
    setError(null);
    try {
      switch (remedy) {
        case "compact": {
          if (!activeId) break;
          setRemedyProgress("Compacting the conversation…");
          await api.compactConversation(activeId);
          setView(await api.conversation(activeId));
          break;
        }
        case "update": {
          const targetId = resolveInstallId(providerHint);
          if (!targetId) {
            setError("No provider is selected to update.");
            break;
          }
          setRemedyProgress("Downloading and installing the latest version…");
          await api.installProvider(targetId);
          setRemedyProgress("Provider ready. Retrying your request…");
          await regenerate({ force: true });
          break;
        }
        case "switch_provider":
          setModelOpen(true);
          break;
        case "retry":
          setRemedyProgress("Retrying…");
          await regenerate({ force: true });
          break;
        case "sign_in":
        default:
          // Signing in happens in the user's own terminal; the card already says how.
          break;
      }
    } catch (remedyError) {
      setError(String((remedyError as Error).message ?? remedyError));
    } finally {
      setBusyRemedy(null);
      setRemedyProgress(null);
    }
  };

  const answerPermission = async (request: PermissionRequest, allow: boolean) => {
    setAnswered((current) => ({ ...current, [request.id]: allow }));
    try {
      await api.respondPermission(request.id, allow);
    } catch (answerError) {
      setError(String((answerError as Error).message ?? answerError));
    }
  };

  const copy = async (turn: ChatTurnView) => {
    try {
      await navigator.clipboard.writeText(turn.content);
      setCopied(turn.id);
      window.setTimeout(() => setCopied((current) => (current === turn.id ? null : current)), 1600);
    } catch {
      setError("Clipboard unavailable.");
    }
  };

  const editMessage = (turn: ChatTurnView) => {
    setInput(turn.content);
    composerRef.current?.focus();
  };

  const onKeyDown = (event: React.KeyboardEvent<HTMLTextAreaElement>) => {
    if (event.key === "Tab" && prediction && !showSlashMenu && !showSkillMenu) {
      event.preventDefault();
      setInput((current) => current + prediction);
      return;
    }
    if (showSlashMenu) {
      if (event.key === "ArrowDown") {
        event.preventDefault();
        setMenuIndex((idx) => (idx + 1) % slashSuggestions.length);
        return;
      }
      if (event.key === "ArrowUp") {
        event.preventDefault();
        setMenuIndex((idx) => (idx - 1 + slashSuggestions.length) % slashSuggestions.length);
        return;
      }
      if (event.key === "Enter") {
        event.preventDefault();
        const selected = slashSuggestions[menuIndex] ?? slashSuggestions[0];
        if (selected) {
          const cmd = selected.cmd;
          if (cmd === "/computer") {
            selectSlashCommand(cmd);
          } else {
            setInput("");
            void sendText(cmd);
          }
        }
        return;
      }
      if (event.key === "Tab") {
        event.preventDefault();
        const selected = slashSuggestions[menuIndex] ?? slashSuggestions[0];
        if (selected) selectSlashCommand(selected.cmd);
        return;
      }
      if (event.key === "Escape") {
        event.preventDefault();
        setInput("");
        return;
      }
    }

    if (showSkillMenu) {
      if (event.key === "ArrowDown") {
        event.preventDefault();
        setMenuIndex((idx) => (idx + 1) % skillSuggestions.length);
        return;
      }
      if (event.key === "ArrowUp") {
        event.preventDefault();
        setMenuIndex((idx) => (idx - 1 + skillSuggestions.length) % skillSuggestions.length);
        return;
      }
      if (event.key === "Enter" || event.key === "Tab") {
        event.preventDefault();
        const selected = skillSuggestions[menuIndex] ?? skillSuggestions[0];
        if (selected) selectSkill(selected);
        return;
      }
      if (event.key === "Escape") {
        event.preventDefault();
        return;
      }
    }

    if (event.key === "Enter" && !event.shiftKey) {
      event.preventDefault();
      if (input.trim()) void send();
      else if (activeAssistant) void stop();
      return;
    }
    if (event.key === "Escape" && activeAssistant) {
      event.preventDefault();
      void stop();
    }
  };

  useEffect(() => {
    let lastEscapeAt = 0;
    const onKey = (event: KeyboardEvent) => {
      if (event.key !== "Escape") return;
      const now = Date.now();
      const twice = lastEscapeAt > 0 && now - lastEscapeAt <= 900;
      lastEscapeAt = now;
      if (!twice) return;
      if (!activeAssistant && !sending) return;
      event.preventDefault();
      void stop();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [activeAssistant, sending]);

  // Auto-grow the composer.
  useEffect(() => {
    const area = composerRef.current;
    if (!area) return;
    area.style.height = "auto";
    area.style.height = `${Math.min(area.scrollHeight, 200)}px`;
  }, [input]);

    const isPristine = turns.length === 0 && !sending;

    return (
      <div className={`chat${isPristine ? " pristine" : " active-session"}${focusMode ? " focus-mode" : ""}`}>
        <section className="thread-wrap" aria-label="Conversation">
          {(turns.length > 0 || view) && (onNewConversation || onCloseConversation) && (
            <div className="chat-top-bar">
              <span className="chat-top-session-name">
                {view?.meta.title || "Active Session"}
              </span>
              <div className="chat-top-actions">
                {onNewConversation ? (
                  <button
                    type="button"
                    className="chat-top-new-btn"
                    onClick={onNewConversation}
                    title="Start a new chat (Ctrl+N)"
                  >
                    <IconPlus size={11} /> New Chat
                  </button>
                ) : null}
                <div className="chat-top-plugins">
                  {plugins
                    .filter((plugin) => plugin.activated)
                    .map((plugin) => {
                      const window = plugin.window;
                      return (
                        <button
                          key={plugin.id}
                          className="chat-top-plugin-btn"
                          title={`${plugin.name} plugin`}
                          onClick={() => {
                            if (window) {
                              const { title, width, height, url } = window;
                              // The handle is not kept: the window owns its own lifetime
                              // and closing it is the user's business, not ours.
                              void new WebviewWindow(`plugin-${plugin.id}-${Date.now()}`, {
                                title,
                                width,
                                height,
                                url,
                              });
                            }
                          }}
                        >
                          <IconGear size={12} />
                        </button>
                      );
                    })}
                </div>
                {onCloseConversation ? (
                  <button
                    type="button"
                    className="chat-top-close-btn"
                    onClick={onCloseConversation}
                    title="Close this chat"
                    aria-label="Close this chat"
                  >
                    <IconClose size={12} />
                  </button>
                ) : null}
              </div>
            </div>
          )}
          <div className="thread" ref={threadRef} onScroll={onScroll}>
            {turns.length === 0 ? (
              <div className="thread-inner thread-inner-empty">
                <ChatWelcome
                  project={project}
                  projects={projects}
                  onSelectProject={onSelectProject}
                  onSelectPrompt={handleSelectPrompt}
                />
              </div>
            ) : (
              <div className="thread-inner">
                {turns.map((turn) => (
                  <TurnRow
                    key={turn.id}
                    turn={turn}
                    isLastAssistant={
                      turn.role === "assistant" && turn.id === turns[turns.length - 1]?.id
                    }
                    answeredMap={answered}
                    copiedId={copied}
                    onAllow={() => void answerPermission(turn.permission as PermissionRequest, true)}
                    onDeny={() => void answerPermission(turn.permission as PermissionRequest, false)}
                    onRegenerate={() => void regenerate()}
                    onCopy={() => void copy(turn)}
                    onEdit={() => editMessage(turn)}
                    onRemedy={(remedy) =>
                      void applyRemedy(remedy, turn.fault?.provider ?? turn.provider)
                    }
                    busyRemedy={busyRemedy}
                    remedyProgress={remedyProgress}
                    liveComputerLabel={
                      turn.id === activeAssistant?.id ? (phase?.label ?? null) : null
                    }
                    computerFullAccess={
                      computerBrowser && permissionMode === "full_access"
                    }
                    onOpenBrowser={onOpenBrowser}
                    onOpenChrome={openLocalInChrome}
                    onReviewTurn={(target) =>
                      onOpenReview?.(clipName(target.content || "this turn", 60))
                    }
                    onUndoTurn={undoableTurns[turn.id] ? (target) => void undoTurn(target) : undefined}
                    undoingTurnId={undoingTurn}
                  />
                ))}
                {phase && activeAssistant && !isComputerPhaseLabel(phase.label) ? (
                  <div className="phase-row">
                    <PhaseIndicator
                      phase={phase.kind}
                      label={phase.label}
                      since={phase.since}
                    />
                  </div>
                ) : null}
                {turns.length > 0 && onOpenReview ? (
                  <div className="thread-bottom-review-bar">
                    <div className="review-bar-left">
                      <IconFile size={14} />
                      <span>0 Files With Changes</span>
                    </div>
                    <button
                      type="button"
                      className="review-changes-pill-btn"
                      onClick={() => onOpenReview(null)}
                      title="Review all changes made in this workspace"
                    >
                      <IconGitMerge size={12} /> Review Changes
                    </button>
                  </div>
                ) : null}
              </div>
            )}
          </div>

          {!stickToBottom.current && turns.length > 0 ? (
            <button className="scroll-pill" onClick={() => scrollToBottom()}>
              <IconChevronDown /> latest
            </button>
          ) : null}

          <div className="composer-zone">
            {error ? (
              <div className="error-inline m-fall" role="alert">
                {error}
              </div>
            ) : null}

            {previewOffer ? (
              <div className="localhost-offer" role="dialog" aria-label="Open local site">
                <div className="localhost-offer-copy">
                  <strong>Local site is ready</strong>
                  <span>{previewOffer.url}</span>
                </div>
                <div className="localhost-offer-actions">
                  <button
                    type="button"
                    className="localhost-offer-btn primary"
                    onClick={() => openLocalInBhippi(previewOffer.url)}
                  >
                    <IconBrowser size={13} />
                    Bhippi browser
                  </button>
                  <button
                    type="button"
                    className="localhost-offer-btn"
                    onClick={() => openLocalInChrome(previewOffer.url)}
                  >
                    <IconExternalLink size={13} />
                    Chrome
                  </button>
                  <button
                    type="button"
                    className="localhost-offer-btn ghost"
                    onClick={() => setPreviewOffer(null)}
                  >
                    Not now
                  </button>
                </div>
              </div>
            ) : null}

            {queuedMessages.length > 0 ? (
              <div className="queued-messages-wrap" role="region" aria-label="Queued messages">
                <div
                  className="queued-messages-head"
                  onClick={() => setIsQueueCollapsed((prev) => !prev)}
                  role="button"
                  tabIndex={0}
                  aria-expanded={!isQueueCollapsed}
                >
                  <div className="queued-head-left">
                    <strong>Queued Messages</strong>
                    <span className="queued-count-badge">{queuedMessages.length}</span>
                    <span className="queued-subtitle">Sends after agent finishes working</span>
                  </div>
                  <button
                    type="button"
                    className={`queued-toggle-btn${isQueueCollapsed ? " collapsed" : ""}`}
                    onClick={(e) => {
                      e.stopPropagation();
                      setIsQueueCollapsed((prev) => !prev);
                    }}
                    aria-label={isQueueCollapsed ? "Expand queue" : "Collapse queue"}
                  >
                    <IconChevronDown size={14} />
                  </button>
                </div>

                {!isQueueCollapsed ? (
                  <div className="queued-messages-list">
                    {queuedMessages.map((item) => (
                      <div key={item.id} className="queued-card">
                        <div className="queued-text" title={item.text}>
                          {item.text}
                        </div>
                        <div className="queued-actions">
                          <button
                            type="button"
                            className="queued-btn fork"
                            title="Fork into a new conversation"
                            aria-label="Fork conversation"
                            onClick={() => void forkQueued(item.id)}
                          >
                            <IconSplitView size={13} />
                          </button>
                          <button
                            type="button"
                            className="queued-btn edit"
                            title="Edit message"
                            aria-label="Edit message"
                            onClick={() => editQueued(item.id)}
                          >
                            <IconEdit size={13} />
                          </button>
                          <button
                            type="button"
                            className="queued-btn copy"
                            title="Copy message"
                            aria-label="Copy message"
                            onClick={() => void copyQueued(item.id)}
                          >
                            {copiedQueuedId === item.id ? (
                              <IconCheck size={13} />
                            ) : (
                              <IconCopy size={13} />
                            )}
                          </button>
                          <button
                            type="button"
                            className="queued-btn delete"
                            title="Delete from queue"
                            aria-label="Delete message"
                            onClick={() => deleteQueued(item.id)}
                          >
                            <IconTrash size={13} />
                          </button>
                          <button
                            type="button"
                            className="queued-btn send-now"
                            title="Send now (interrupt current turn)"
                            aria-label="Send now"
                            onClick={() => void sendNow(item.id)}
                          >
                            <IconArrowRight size={13} />
                          </button>
                        </div>
                      </div>
                    ))}
                  </div>
                ) : null}
              </div>
            ) : null}

            {/* Unified composer + activity shell — flush card with rounded corners */}
            <div className={`composer-shell${streaming ? " is-working" : ""}`}>
              {/* Live Coding Activity Bar directly above Composer */}
              {!activeAssistant?.tools.some((tool) => tool.action === "control_computer") ? (
                <ActivityDock
                  tools={activeAssistant?.tools ?? []}
                  thinking={activeAssistant?.thinking ?? null}
                  thinkingElapsedMs={activeAssistant?.thinking_elapsed_ms ?? null}
                  phase={
                    phase && activeAssistant
                      ? { label: phase.label, since: phase.since, kind: phase.kind }
                      : null
                  }
                  streaming={streaming}
                  permission={activeAssistant?.permission ?? null}
                  answered={
                    activeAssistant?.permission
                      ? answered[activeAssistant.permission.id] !== undefined
                      : false
                  }
                  onAllow={() =>
                    activeAssistant?.permission &&
                    void answerPermission(activeAssistant.permission, true)
                  }
                  onDeny={() =>
                    activeAssistant?.permission &&
                    void answerPermission(activeAssistant.permission, false)
                  }
                />
              ) : null}

            <div className="composer" style={{ position: "relative" }}>
              {/* Autocomplete Popover */}
              {showSlashMenu ? (
                <div className="composer-autocomplete-popover command-palette" role="listbox" aria-label="Slash commands">
                  <div className="command-palette-head" aria-hidden="true">
                    <span>Commands</span>
                    <span className="command-palette-hint">↑↓ navigate · Enter select</span>
                  </div>
                  <div className="command-palette-section" aria-hidden="true">Available now</div>
                  <div className="command-palette-list">
                    {slashSuggestions.map((cmd, idx) => (
                      <button
                        key={cmd.cmd}
                        type="button"
                        role="option"
                        aria-selected={menuIndex === idx}
                        className={`autocomplete-item${menuIndex === idx ? " active" : ""}`}
                        onClick={() => {
                          if (cmd.cmd === "/computer") {
                            selectSlashCommand(cmd.cmd);
                          } else {
                            setInput("");
                            void sendText(cmd.cmd);
                          }
                        }}
                        onMouseEnter={() => setMenuIndex(idx)}
                      >
                        <span className="autocomplete-icon"><SlashCommandIcon kind={cmd.icon} /></span>
                        <div className="autocomplete-content">
                          <div className="autocomplete-title">
                            <code>{cmd.cmd}</code>
                            <span>{cmd.label}</span>
                          </div>
                          <div className="autocomplete-desc">{cmd.desc}</div>
                        </div>
                        <span className="autocomplete-kind">Command</span>
                      </button>
                    ))}
                  </div>
                </div>
              ) : null}

              {showSkillMenu ? (
                <div className="composer-autocomplete-popover" role="listbox" aria-label="AI skills">
                  {skillSuggestions.map((sk, idx) => (
                    <button
                      key={sk.id}
                      type="button"
                      role="option"
                      aria-selected={menuIndex === idx}
                      className={`autocomplete-item${menuIndex === idx ? " active" : ""}`}
                      onClick={() => selectSkill(sk)}
                      onMouseEnter={() => setMenuIndex(idx)}
                    >
                      <span className="autocomplete-icon"><IconBolt size={13} /></span>
                      <div className="autocomplete-content">
                        <div className="autocomplete-title">
                          <code>@{sk.id}</code>
                          <span>{sk.name}</span>
                          <span className="matrix-chip supported" style={{ fontSize: "10px", padding: "1px 5px" }}>
                            {sk.source}
                          </span>
                        </div>
                        <div className="autocomplete-desc">{sk.description}</div>
                      </div>
                    </button>
                  ))}
                </div>
              ) : null}

              {isComputerIntent && !hasVision ? (
                <div className="vision-warning-banner" role="alert">
                  <div className="vision-warning-icon">
                    <IconVision size={15} />
                  </div>
                  <div className="vision-warning-body">
                    <strong>Multimodal Vision Required</strong>
                    <span>
                      Active model (<code>{currentModel || providerId || "current"}</code>) lacks vision reasoning.
                      Switch to a vision model (e.g., Claude 3.5 Sonnet, GPT-4o, Gemini 1.5 Pro, Qwen 2.5-VL) to view your desktop.
                    </span>
                  </div>
                  <button
                    type="button"
                    className="btn-switch-model"
                    onClick={() => {
                      setModelOpen(true);
                    }}
                  >
                    Switch Model
                  </button>
                </div>
              ) : null}

              <div className="composer-context">
                <span className="context-chip project" title={`${project.name}\n${project.path}`}>
                  {clipName(project.name)}
                </span>
                {project.is_git_repository ? (
                  <span className="context-chip" title="Current Git branch">
                    branch · {project.branch ?? "repository"}
                  </span>
                ) : null}
                {taggedSkills.map((sk) => (
                  <span
                    key={sk.id}
                    className="context-chip skill"
                    title={`Active skill: ${sk.description}`}
                  >
                    <IconBolt size={10} /> {sk.name}
                  </span>
                ))}
              </div>
              {isRecording ? (
                <div className="composer-voice-indicator">
                  <span className="voice-live-dot" />
                  <span className="voice-provider-label">
                    Listening via <strong>{activeAudioConfig?.name ?? "Speech API"}</strong> (
                    {Math.floor(recordingSeconds / 60)}:
                    {(recordingSeconds % 60).toString().padStart(2, "0")})
                  </span>
                  <span className="voice-hint">Click mic button to finish</span>
                </div>
              ) : isTranscribing ? (
                <div className="composer-voice-indicator transcribing">
                  <IconRefresh size={12} className="spin" />
                  <span>Transcribing voice via {activeAudioConfig?.name ?? "Audio Engine"}…</span>
                </div>
              ) : null}

              <div className="composer-input">
                <textarea
                  ref={composerRef}
                  value={input}
                  rows={1}
                  placeholder={
                    isRecording
                      ? "Listening to voice input…"
                      : isTranscribing
                        ? "Transcribing voice…"
                        : activeAssistant
                          ? "Bhippi is answering — type to queue message or Esc to stop…"
                          : "Ask anything"
                  }
                  onChange={(event) => {
                    const next = event.target.value;
                    setInput(next);
                    if (activeId) conversationDrafts.set(activeId, next);
                    setMenuIndex(0);
                  }}
                  onKeyDown={onKeyDown}
                  aria-label="Message"
                />
                {prediction ? (
                  <div className="composer-prediction">
                    <kbd>Tab</kbd>
                    <span>
                      {input}
                      <span className="prediction-ghost">{prediction}</span>
                    </span>
                  </div>
                ) : null}

                <div className="composer-action-group">
                  {/* Setup popover if key is missing */}
                  {audioPromptOpen ? (
                    <div className="audio-mic-popover" role="dialog" aria-label="Audio Configuration">
                      <div className="audio-mic-popover-head">
                        <IconMic size={14} />
                        <strong>Voice Input Setup</strong>
                        <button
                          type="button"
                          className="audio-popover-close-btn"
                          onClick={() => setAudioPromptOpen(false)}
                          aria-label="Close"
                        >
                          <IconClose size={11} />
                        </button>
                      </div>
                      <p className="audio-mic-popover-desc">
                        No API key found for <strong>{activeAudioConfig?.name || "Audio Service"}</strong>.
                        Add your key in Settings to use Deepgram / Whisper, or switch to browser speech recognition.
                      </p>
                      <div className="audio-mic-popover-actions">
                        <button
                          type="button"
                          className="btn-mic-popover-primary"
                          onClick={() => {
                            setAudioPromptOpen(false);
                            onOpenSettings?.("Audio & Voice");
                          }}
                        >
                          Open Audio Settings
                        </button>
                        <button
                          type="button"
                          className="btn-mic-popover-secondary"
                          onClick={() => {
                            setAudioPromptOpen(false);
                            saveAudioSettings({ activeSttProvider: "web_speech" });
                            setTimeout(() => {
                              void startVoiceRecording();
                            }, 50);
                          }}
                        >
                          Use Browser Speech
                        </button>
                      </div>
                    </div>
                  ) : null}

                  {streaming && input.trim() ? (
                    <button
                      type="button"
                      className="composer-circle-send queue"
                      onClick={() => void send()}
                      title="Add to queue — sends after this turn finishes"
                      aria-label="Add to queue"
                    >
                      <IconQueue size={13} />
                    </button>
                  ) : streaming ? (
                    <button
                      type="button"
                      className="composer-circle-send stop"
                      onClick={() => void stop()}
                      title="Stop response (Esc)"
                      aria-label="Stop response"
                    >
                      <IconStop size={12} />
                    </button>
                  ) : (
                    <button
                      type="button"
                      className="composer-circle-send"
                      onClick={() => void send()}
                      title="Send message (Enter)"
                      aria-label="Send message"
                      disabled={!input.trim() || sending}
                    >
                      <IconArrowUp size={15} />
                    </button>
                  )}
                </div>
            </div>

            {/* Bottom Toolbar Strip (Matches Screenshots 1, 2, 3, 4, 5) */}
            <div className="composer-bar">
              {/* 1. Permission Mode Trigger & Popover (Screenshot 5) */}
              <PermissionPopover
                mode={permissionMode}
                computerBrowser={computerBrowser}
                open={permissionOpen}
                onOpenChange={(next) => {
                  setPermissionOpen(next);
                  if (next) {
                    setProviderOpen(false);
                    setModelOpen(false);
                    setThinkingOpen(false);
                    setAddMenuOpen(false);
                  }
                }}
                onSelectMode={(mode) => {
                  setPermissionMode(mode);
                  setAgentMode(mode === "auto" || mode === "full_access");
                }}
                onToggleComputerBrowser={() => toggleComputerBrowser(!computerBrowser)}
              />

              {/* 2. Provider Trigger & Popover (Screenshot 1) */}
              <ProviderPopover
                providers={chatOptions}
                currentId={currentOption?.id ?? null}
                open={providerOpen}
                onOpenChange={(next) => {
                  setProviderOpen(next);
                  if (next) {
                    setPermissionOpen(false);
                    setModelOpen(false);
                    setThinkingOpen(false);
                    setAddMenuOpen(false);
                  }
                }}
                onSelect={(id) => {
                  chooseProvider(id);
                }}
              />

              {/* 3. Model Trigger & Popover (Screenshots 2 & 4) */}
              <ModelPopover
                provider={currentOption}
                currentModel={currentModel}
                open={modelOpen}
                onOpenChange={(next) => {
                  setModelOpen(next);
                  if (next) {
                    setPermissionOpen(false);
                    setProviderOpen(false);
                    setThinkingOpen(false);
                    setAddMenuOpen(false);
                  }
                }}
                onSelect={chooseModel}
              />

              {/* 4. Thinking / Effort Trigger & Popover (Screenshot 3) */}
              <ThinkingPopover
                effort={effort}
                open={thinkingOpen}
                onOpenChange={(next) => {
                  setThinkingOpen(next);
                  if (next) {
                    setPermissionOpen(false);
                    setProviderOpen(false);
                    setModelOpen(false);
                    setAddMenuOpen(false);
                  }
                }}
                onSelect={chooseEffort}
              />

              {/* 5. Computer Use / Perception Indicator */}
              <button
                type="button"
                className={`composer-bar-btn dot-trigger${computerBrowser ? " active" : ""}`}
                onClick={() => toggleComputerBrowser(!computerBrowser)}
                title={computerBrowser ? "Computer perception active" : "Enable computer perception"}
                aria-label="Computer perception"
              >
                <IconMonitor size={15} />
              </button>

              <span className="grow" />

              {/* 6. Settings & Options Popover (Screenshot 2) */}
              <OptionsPopover
                open={addMenuOpen}
                onOpenChange={(next) => {
                  setAddMenuOpen(next);
                  if (next) {
                    setPermissionOpen(false);
                    setProviderOpen(false);
                    setModelOpen(false);
                    setThinkingOpen(false);
                    setUsageOpen(false);
                  }
                }}
                onAttach={() => {
                  setInput((prev) => (prev ? `${prev} @` : "@"));
                  composerRef.current?.focus();
                }}
                designOn={designOn}
                onToggleDesign={toggleDesign}
                focusMode={focusMode}
                onToggleFocus={() => setFocusMode(!focusMode)}
                agentMode={agentMode}
                onToggleAgentMode={() => {
                  setAgentMode((on) => {
                    const next = !on;
                    setPermissionMode((mode) => {
                      if (next) return mode === "full_access" ? "full_access" : "auto";
                      return "ask_approval";
                    });
                    return next;
                  });
                }}
                predictiveText={predictiveText}
                onTogglePredictiveText={() => setPredictiveText(!predictiveText)}
                indexMapOn={indexMapOn}
                onToggleIndexMap={() => setIndexMapOn((prev) => {
                  const next = !prev;
                  try {
                    localStorage.setItem("bhippi_index_map", next ? "on" : "off");
                  } catch {
                    // ignore
                  }
                  return next;
                })}
                caveman={cavemanOn}
                onToggleCaveman={() => setCavemanOn((prev) => {
                  const next = !prev;
                  try {
                    localStorage.setItem("bhippi_caveman_mode", next ? "on" : "off");
                  } catch {
                    // ignore
                  }
                  return next;
                })}
                fontSize={fontSize}
                onChangeFontSize={setFontSize}
              />

              {/* 7. Usage Meter (Screenshot 1) */}
              <ChatUsageMeter
                provider={currentOption}
                currentModel={currentModel}
                summary={usage ?? null}
                limits={limits}
                open={usageOpen}
                onOpenChange={setUsageOpen}
                onRefresh={onRefreshUsage}
                onManage={onManageUsage}
              />

              {/* 8. Microphone Button */}
              <button
                type="button"
                className={`tool-btn mic${isRecording ? " recording" : ""}${isTranscribing ? " transcribing" : ""}`}
                onClick={handleToggleMic}
                title={
                  isRecording
                    ? "Stop recording and transcribe"
                    : isTranscribing
                      ? "Transcribing voice…"
                      : `Voice Input (${activeAudioConfig?.name ?? "Speech API"})`
                }
                aria-label="Voice input"
              >
                <IconMic size={15} />
                {isRecording ? <span className="mic-pulse-ring" /> : null}
              </button>
            </div>
            </div>
          </div>
        </div>
      </section>
    </div>
  );
}

export function extractThinking(
  rawContent: string,
  explicitThinking?: string | null,
): { thinking: string | null; content: string } {
  let thinking = explicitThinking ?? null;
  let content = rawContent;

  // 1. Extract <think>...</think> or streaming unclosed <think>...
  const thinkStartIdx = content.indexOf("<think>");
  if (thinkStartIdx >= 0) {
    const thinkEndIdx = content.indexOf("</think>", thinkStartIdx);
    if (thinkEndIdx >= 0) {
      const extracted = content.slice(thinkStartIdx + 7, thinkEndIdx).trim();
      thinking = thinking ? `${thinking}\n${extracted}` : extracted;
      content = (content.slice(0, thinkStartIdx) + content.slice(thinkEndIdx + 8)).trim();
    } else {
      // Currently streaming inside <think>
      const extracted = content.slice(thinkStartIdx + 7).trim();
      thinking = thinking ? `${thinking}\n${extracted}` : extracted;
      content = content.slice(0, thinkStartIdx).trim();
    }
  }

  // 2. Extract leading Thinking Process blocks if model formats that way
  if (!thinking) {
    const match = content.match(
      /^(?:Thinking Process|Thought Process|Internal Reasoning):\s*\n([\s\S]*?)\n\n([\s\S]*)$/i,
    );
    if (match) {
      thinking = match[1].trim();
      content = match[2].trim();
    }
  }

  return { thinking: thinking && thinking.trim() ? thinking.trim() : null, content };
}

function ThinkingAccordion({
  thinking,
  elapsedMs,
  isStreaming,
}: {
  thinking: string;
  elapsedMs?: number | null;
  isStreaming?: boolean;
}) {
  const [isOpen, setIsOpen] = useState(false);

  const seconds = elapsedMs ? Math.max(1, Math.round(elapsedMs / 1000)) : null;
  const label = isStreaming
    ? "Thinking..."
    : seconds
      ? seconds >= 60
        ? `Worked for ${Math.round(seconds / 60)}m`
        : `Thought for ${seconds}s`
      : "Thought for a few seconds";

  return (
    <div className={`thinking-accordion${isOpen ? " open" : ""}`}>
      <button
        type="button"
        className="thinking-trigger"
        onClick={() => setIsOpen(!isOpen)}
        aria-expanded={isOpen}
        title={isOpen ? "Collapse thought process" : "Expand thought process"}
      >
        <span className="thinking-label">{label}</span>
        <span className="thinking-chevron" aria-hidden="true">
          ›
        </span>
      </button>
      {isOpen ? (
        <div className="thinking-drawer" role="region" aria-label="Thinking process">
          <div className="thinking-content">{thinking}</div>
        </div>
      ) : null}
    </div>
  );
}

function TurnWorkTree({
  tools,
  thinking,
  elapsedMs,
  isStreaming,
}: {
  tools: ToolActivity[];
  thinking: string | null;
  elapsedMs?: number | null;
  isStreaming: boolean;
}) {
  const [isOpen, setIsOpen] = useState(true);

  if (tools.length === 0 && !thinking && !isStreaming) return null;

  // CHT-110: the header used to always read "Exploring N files", including on a turn that
  // edited twelve files and ran four commands. It now says what the steps actually were.
  const groups = groupTools(tools);
  const headerLabel =
    groups.length === 0
      ? isStreaming
        ? "Working"
        : "Activity"
      : groups.length === 1
        ? groupHeadline(groups[0])
        : `${groups.length} steps`;

  return (
    <div className={`turn-work-tree${isOpen ? " open" : ""}`}>
      <button
        type="button"
        className="turn-work-tree-header"
        onClick={() => setIsOpen(!isOpen)}
        aria-expanded={isOpen}
      >
        <span className="turn-work-tree-title">{headerLabel}</span>
        <span className="turn-work-tree-chev" aria-hidden="true">
          {isOpen ? "▾" : "›"}
        </span>
      </button>

      {isOpen ? (
        <div className="turn-work-tree-body">
          {thinking ? (
            <ThinkingAccordion
              thinking={thinking}
              elapsedMs={elapsedMs}
              isStreaming={isStreaming && tools.length === 0}
            />
          ) : isStreaming && tools.length === 0 ? (
            <div className="thinking-accordion streaming-placeholder">
              <span className="thinking-label">Thinking...</span>
              <span className="thinking-chevron">›</span>
            </div>
          ) : null}

          {groups.map((group, index) => (
            <ActivityGroup
              key={group.id}
              group={group}
              // The last group of a running turn opens itself: the reason to watch a live
              // turn is to see what it is doing now (plan §3, rule 1).
              defaultOpen={isStreaming && index === groups.length - 1}
            />
          ))}

          {isStreaming ? (
            <div className="turn-work-item working">
              <span className="turn-work-working-label">Working...</span>
            </div>
          ) : null}
        </div>
      ) : null}
    </div>
  );
}

type TurnRowProps = {
  turn: ChatTurnView;
  isLastAssistant: boolean;
  answeredMap: Record<string, boolean | undefined>;
  copiedId: string | null;
  onAllow: (request: PermissionRequest) => void;
  onDeny: (request: PermissionRequest) => void;
  onRegenerate: () => void;
  onCopy: () => void;
  onEdit: () => void;
  onRemedy: (remedy: string, providerHint?: string | null) => void;
  busyRemedy: string | null;
  remedyProgress: string | null;
  liveComputerLabel?: string | null;
  computerFullAccess: boolean;
  onOpenBrowser?: (url?: string) => void;
  onOpenChrome?: (url: string) => void;
  /// CHT-116: open the review modal filtered to this turn.
  onReviewTurn?: (turn: ChatTurnView) => void;
  /// CHT-115: put every file this turn changed back. Absent means the action is not offered.
  onUndoTurn?: (turn: ChatTurnView) => void;
  undoingTurnId?: string | null;
};

function TurnRow({
  turn,
  isLastAssistant,
  answeredMap,
  copiedId,
  onAllow,
  onDeny,
  onRegenerate,
  onCopy,
  onEdit,
  onRemedy,
  busyRemedy,
  remedyProgress,
  liveComputerLabel,
  computerFullAccess,
  onOpenBrowser,
  onOpenChrome,
  onReviewTurn,
  onUndoTurn,
  undoingTurnId,
}: TurnRowProps) {
  const { thinking, content: cleanContent } = extractThinking(turn.content, turn.thinking);
  const localhostUrlMatch = firstLocalhostUrl(cleanContent);
  const computerTools = turn.tools.filter((tool) => tool.action === "control_computer");
  const ordinaryTools = turn.tools.filter((tool) => tool.action !== "control_computer");
  const showComputerPanel =
    computerTools.length > 0 ||
    (turn.role === "assistant" && isComputerPhaseLabel(liveComputerLabel));

  return (
    <article
      className={`turn ${turn.role}`}
      aria-label={`${turn.role} message`}
    >
      {turn.permission ? (
        <PermissionCard
          request={turn.permission}
          answered={answeredMap[turn.permission.id]}
          onAllow={() => onAllow(turn.permission as PermissionRequest)}
          onDeny={() => onDeny(turn.permission as PermissionRequest)}
        />
      ) : null}

      {turn.role === "user" ? (
        <div className="user-bubble-card">
          <div className="user-text">{turn.content}</div>
          <div className="user-actions">
            <button
              className="icon-btn"
              onClick={onCopy}
              title="Copy prompt"
              aria-label="Copy prompt"
            >
              {copiedId === turn.id ? <IconCheck size={13} /> : <IconCopy size={13} />}
            </button>
            <button
              className="icon-btn"
              onClick={onEdit}
              title="Reuse prompt"
              aria-label="Reuse prompt"
            >
              <IconEdit size={13} />
            </button>
          </div>
        </div>
      ) : (
        <div className="assistant-turn-body">
          {showComputerPanel ? (
            <BhippiComputerPanel
              tools={computerTools}
              turnState={turn.state}
              fullAccess={computerFullAccess}
              liveLabel={liveComputerLabel}
            />
          ) : null}
          <TurnWorkTree
            tools={ordinaryTools}
            thinking={thinking}
            elapsedMs={turn.thinking_elapsed_ms}
            isStreaming={turn.state === "streaming"}
          />

          {cleanContent ? <Markdown text={cleanContent} /> : null}
          {localhostUrlMatch && (onOpenBrowser || onOpenChrome) ? (
            <div className="turn-browser-preview-banner">
              {onOpenBrowser ? (
                <button
                  type="button"
                  className="turn-preview-btn"
                  onClick={() => onOpenBrowser(localhostUrlMatch)}
                  title="Open in Bhippi browser"
                >
                  <IconBrowser size={13} /> Bhippi browser
                </button>
              ) : null}
              {onOpenChrome ? (
                <button
                  type="button"
                  className="turn-preview-btn secondary"
                  onClick={() => onOpenChrome(localhostUrlMatch)}
                  title="Open in Chrome"
                >
                  <IconExternalLink size={13} /> Chrome
                </button>
              ) : null}
            </div>
          ) : null}
          {turn.state === "streaming" && cleanContent ? (

            <span className="caret" aria-hidden="true" />
          ) : null}
          {turn.fault ? (
            <FaultCard
              fault={turn.fault}
              onAct={onRemedy}
              busy={busyRemedy === turn.fault.remedy}
              status={busyRemedy === turn.fault.remedy ? remedyProgress : null}
            />
          ) : turn.state === "failed" && !turn.content && !thinking ? (
            <span style={{ color: "var(--error)" }}>The turn failed.</span>
          ) : null}
          {turn.state === "stopped" ? (
            <span className="provider-tag" style={{ display: "block", marginTop: 6 }}>
              stopped by you
            </span>
          ) : null}

          {/* CHT-117: what the whole turn cost in wall-clock time, computed in Rust. */}
          {turn.worked_ms && isTerminal(turn.state) ? (
            <div className="turn-worked">Worked for {formatDuration(turn.worked_ms)}</div>
          ) : null}

          {/* CHT-118: usage limits and provider warnings finally have a lane. */}
          <TurnNotices notices={turn.notices ?? []} />

          {/* CHT-114/115/116: what this turn changed, and the two things to do about it. */}
          {turn.changes && turn.changes.files.length > 0 ? (
            <TurnChangesCard
              changes={turn.changes}
              onReview={() => onReviewTurn?.(turn)}
              onUndo={onUndoTurn ? () => onUndoTurn(turn) : undefined}
              undoing={undoingTurnId === turn.id}
              undoDisabledReason={
                isTerminal(turn.state)
                  ? null
                  : "The turn is still running; wait for it to finish before undoing it."
              }
            />
          ) : null}
          {isTerminal(turn.state) && (cleanContent || thinking) ? (
            <div className="assistant-footer-actions">
              <button
                className={`icon-btn${copiedId === turn.id ? " copied" : ""}`}
                onClick={onCopy}
                title="Copy answer"
                aria-label="Copy answer"
              >
                {copiedId === turn.id ? <IconCheck size={13} /> : <IconCopy size={13} />}
              </button>
              {isLastAssistant ? (
                <button
                  className="icon-btn"
                  onClick={onRegenerate}
                  title="Regenerate response"
                  aria-label="Regenerate response"
                >
                  <IconRefresh size={13} />
                </button>
              ) : null}
            </div>
          ) : null}
        </div>
      )}
    </article>
  );
}

function triggerIndexMapIndexing(
  conversationId: string,
  _newMessage: string,
): Promise<void> {
  return new Promise((resolve) => {
    // When IndexMap is ON, we create a lightweight index of the conversation
    // context so the AI can find relevant information quickly without
    // re-exploring the entire codebase. This saves tokens and makes responses faster.
    //
    // TODO: Integrate with FixMap or similar local-first repo mapping tool
    // for detailed file/function/test indexing.
    //
    // For now, we log the indexing action and could invoke a backend RPC
    // or MCP server to perform the actual indexing.
    console.log(
      `[IndexMap] Auto-indexing conversation ${conversationId} with new message`,
    );
    // Placeholder: in a full implementation, this would call a Rust backend
    // command or MCP server to generate a ranked context map, similar to FixMap:
    // - Rank relevant files/functions with confidence scores
    // - Extract test routes related to the new message
    // - Generate risk notes for potential regression points
    resolve();
  });
}

function PermissionCard({
  request,
  answered,
  onAllow,
  onDeny,
}: {
  request: PermissionRequest;
  answered: boolean | undefined;
  onAllow: () => void;
  onDeny: () => void;
}) {
  const settled = answered !== undefined;
  return (
    <div className={`permission${settled ? " answered" : ""}`} role="group" aria-label="Permission request">
      <div className="permission-head">
        <span className="permission-action">{request.action}</span>
        <span className="scope-chip">{request.scope}</span>
        <span className={`scope-chip risk-chip ${request.risk}`}>{request.risk}</span>
      </div>
      <p className="permission-detail">{request.detail}</p>
      {settled ? (
        <div className="permission-answer">
          you answered: {answered ? "allow once" : "deny"}
        </div>
      ) : (
        <div className="permission-buttons">
          <button className="btn-accent" onClick={onAllow}>
            Allow once
          </button>
          <button className="btn-primary" onClick={onDeny}>
            Deny
          </button>
        </div>
      )}
    </div>
  );
}
