export type AudioProviderId =
  | "deepgram"
  | "openai"
  | "groq"
  | "elevenlabs"
  | "assemblyai"
  | "cartesia"
  | "web_speech"
  | "custom";

export interface AudioProviderConfig {
  id: AudioProviderId;
  name: string;
  category: "stt" | "tts" | "both";
  apiKey: string;
  model: string;
  baseUrl: string;
  enabled: boolean;
  docsUrl: string;
  description: string;
  defaultModel: string;
  supportedModels: string[];
}

export interface AudioSettings {
  activeSttProvider: AudioProviderId;
  language: string;
  autoPunctuation: boolean;
  providers: Record<AudioProviderId, AudioProviderConfig>;
}

export const DEFAULT_AUDIO_SETTINGS: AudioSettings = {
  activeSttProvider: "deepgram",
  language: "en",
  autoPunctuation: true,
  providers: {
    deepgram: {
      id: "deepgram",
      name: "Deepgram",
      category: "both",
      apiKey: "",
      model: "nova-3",
      baseUrl: "https://api.deepgram.com",
      enabled: true,
      docsUrl: "https://console.deepgram.com",
      description: "Industry-leading real-time & batch speech recognition (Nova-3 / Nova-2) & Aura text-to-speech.",
      defaultModel: "nova-3",
      supportedModels: ["nova-3", "nova-2", "nova-2-general", "nova-2-meeting", "nova-2-conversationalai"],
    },
    openai: {
      id: "openai",
      name: "OpenAI Audio",
      category: "both",
      apiKey: "",
      model: "whisper-1",
      baseUrl: "https://api.openai.com/v1",
      enabled: true,
      docsUrl: "https://platform.openai.com/api-keys",
      description: "Whisper speech-to-text with multilingual transcription & OpenAI neural TTS voices.",
      defaultModel: "whisper-1",
      supportedModels: ["whisper-1", "gpt-4o-audio-preview", "tts-1", "tts-1-hd"],
    },
    groq: {
      id: "groq",
      name: "Groq Whisper",
      category: "stt",
      apiKey: "",
      model: "whisper-large-v3-turbo",
      baseUrl: "https://api.groq.com/openai/v1",
      enabled: true,
      docsUrl: "https://console.groq.com/keys",
      description: "Ultra-fast LPU-accelerated Whisper transcription with sub-second turnaround.",
      defaultModel: "whisper-large-v3-turbo",
      supportedModels: ["whisper-large-v3-turbo", "whisper-large-v3", "distil-whisper-large-v3-en"],
    },
    elevenlabs: {
      id: "elevenlabs",
      name: "ElevenLabs",
      category: "tts",
      apiKey: "",
      model: "eleven_multilingual_v2",
      baseUrl: "https://api.elevenlabs.io/v1",
      enabled: false,
      docsUrl: "https://elevenlabs.io/app/settings/api-keys",
      description: "Lifelike conversational voice synthesis and emotional speech generation.",
      defaultModel: "eleven_multilingual_v2",
      supportedModels: ["eleven_multilingual_v2", "eleven_turbo_v2_5", "eleven_flash_v2_5"],
    },
    assemblyai: {
      id: "assemblyai",
      name: "AssemblyAI",
      category: "stt",
      apiKey: "",
      model: "best",
      baseUrl: "https://api.assemblyai.com/v2",
      enabled: false,
      docsUrl: "https://www.assemblyai.com/app",
      description: "Production-grade speech recognition with deep entity detection and formatting.",
      defaultModel: "best",
      supportedModels: ["best", "nano"],
    },
    cartesia: {
      id: "cartesia",
      name: "Cartesia Sonic",
      category: "tts",
      apiKey: "",
      model: "sonic-english",
      baseUrl: "https://api.cartesia.ai",
      enabled: false,
      docsUrl: "https://play.cartesia.ai/keys",
      description: "Sub-100ms ultra-low latency streaming voice engine.",
      defaultModel: "sonic-english",
      supportedModels: ["sonic-english", "sonic-multilingual"],
    },
    web_speech: {
      id: "web_speech",
      name: "Web Speech API (Browser Native)",
      category: "stt",
      apiKey: "",
      model: "native",
      baseUrl: "",
      enabled: true,
      docsUrl: "https://developer.mozilla.org/en-US/docs/Web/API/Web_Speech_API",
      description: "Built-in browser recognition engine. Zero setup and no API key required.",
      defaultModel: "native",
      supportedModels: ["native"],
    },
    custom: {
      id: "custom",
      name: "Custom Audio Endpoint",
      category: "both",
      apiKey: "",
      model: "whisper",
      baseUrl: "http://localhost:8000/v1",
      enabled: false,
      docsUrl: "",
      description: "Connect to your own self-hosted Whisper server, Faster-Whisper, or vLLM Audio endpoint.",
      defaultModel: "whisper",
      supportedModels: ["whisper", "custom"],
    },
  },
};

const STORAGE_KEY = "bhippi-audio-settings";
const AUDIO_CHANGE_EVENT = "bhippi-audio-changed";

export function getAudioSettings(): AudioSettings {
  try {
    const raw = window.localStorage.getItem(STORAGE_KEY);
    if (!raw) return DEFAULT_AUDIO_SETTINGS;
    const parsed = JSON.parse(raw);
    return {
      ...DEFAULT_AUDIO_SETTINGS,
      ...parsed,
      providers: {
        ...DEFAULT_AUDIO_SETTINGS.providers,
        ...(parsed.providers || {}),
      },
    };
  } catch {
    return DEFAULT_AUDIO_SETTINGS;
  }
}

export function saveAudioSettings(partial: Partial<AudioSettings>): AudioSettings {
  const current = getAudioSettings();
  const next: AudioSettings = {
    ...current,
    ...partial,
    providers: {
      ...current.providers,
      ...(partial.providers || {}),
    },
  };
  try {
    window.localStorage.setItem(STORAGE_KEY, JSON.stringify(next));
    window.dispatchEvent(new CustomEvent(AUDIO_CHANGE_EVENT, { detail: next }));
  } catch (err) {
    console.error("Failed to save audio settings:", err);
  }
  return next;
}

export function onAudioSettingsChange(callback: (settings: AudioSettings) => void): () => void {
  const handler = (e: Event) => {
    const custom = e as CustomEvent<AudioSettings>;
    callback(custom.detail || getAudioSettings());
  };
  window.addEventListener(AUDIO_CHANGE_EVENT, handler);
  window.addEventListener("storage", () => callback(getAudioSettings()));
  return () => {
    window.removeEventListener(AUDIO_CHANGE_EVENT, handler);
  };
}

export function maskApiKey(key: string): string {
  if (!key) return "";
  if (key.length <= 8) return "••••••••";
  return `${key.slice(0, 4)}••••••••${key.slice(-4)}`;
}

export async function testAudioProvider(
  id: AudioProviderId,
  apiKey: string,
  baseUrl?: string,
): Promise<{ success: boolean; message: string }> {
  if (id === "web_speech") {
    const supported =
      typeof window !== "undefined" &&
      ("webkitSpeechRecognition" in window || "SpeechRecognition" in window);
    return supported
      ? { success: true, message: "Browser Speech Recognition is available on this system." }
      : { success: false, message: "Web Speech API is not supported in this browser window." };
  }

  if (!apiKey.trim()) {
    return { success: false, message: "Please provide an API key to test." };
  }

  try {
    if (id === "deepgram") {
      // Validate key using Deepgram projects endpoint
      const res = await fetch("https://api.deepgram.com/v1/projects", {
        headers: {
          Authorization: `Token ${apiKey.trim()}`,
        },
      });
      if (res.ok) {
        return { success: true, message: "Deepgram API connection verified successfully!" };
      }
      const errData = await res.json().catch(() => null);
      const msg = errData?.err_msg || errData?.message || `HTTP ${res.status}: ${res.statusText}`;
      return { success: false, message: `Deepgram verification failed: ${msg}` };
    }

    if (id === "openai") {
      // Validate key using OpenAI models endpoint
      const url = `${(baseUrl || "https://api.openai.com/v1").replace(/\/$/, "")}/models`;
      const res = await fetch(url, {
        headers: {
          Authorization: `Bearer ${apiKey.trim()}`,
        },
      });
      if (res.ok) {
        return { success: true, message: "OpenAI API connection verified successfully!" };
      }
      const errData = await res.json().catch(() => null);
      const msg = errData?.error?.message || `HTTP ${res.status}: ${res.statusText}`;
      return { success: false, message: `OpenAI verification failed: ${msg}` };
    }

    if (id === "groq") {
      // Validate key using Groq models endpoint
      const url = `${(baseUrl || "https://api.groq.com/openai/v1").replace(/\/$/, "")}/models`;
      const res = await fetch(url, {
        headers: {
          Authorization: `Bearer ${apiKey.trim()}`,
        },
      });
      if (res.ok) {
        return { success: true, message: "Groq Whisper API connection verified successfully!" };
      }
      const errData = await res.json().catch(() => null);
      const msg = errData?.error?.message || `HTTP ${res.status}: ${res.statusText}`;
      return { success: false, message: `Groq verification failed: ${msg}` };
    }

    if (id === "elevenlabs") {
      const res = await fetch("https://api.elevenlabs.io/v1/user", {
        headers: {
          "xi-api-key": apiKey.trim(),
        },
      });
      if (res.ok) {
        return { success: true, message: "ElevenLabs API connection verified successfully!" };
      }
      return { success: false, message: `ElevenLabs verification failed (HTTP ${res.status}).` };
    }

    if (id === "assemblyai") {
      const res = await fetch("https://api.assemblyai.com/v2/transcript", {
        method: "GET",
        headers: {
          Authorization: apiKey.trim(),
        },
      });
      if (res.ok || res.status === 200 || res.status === 400) {
        return { success: true, message: "AssemblyAI API connection verified successfully!" };
      }
      return { success: false, message: `AssemblyAI verification returned HTTP ${res.status}.` };
    }

    if (id === "cartesia") {
      const res = await fetch("https://api.cartesia.ai/voices", {
        headers: {
          "X-API-Key": apiKey.trim(),
          "Cartesia-Version": "2024-06-10",
        },
      });
      if (res.ok) {
        return { success: true, message: "Cartesia API connection verified successfully!" };
      }
      return { success: false, message: `Cartesia verification returned HTTP ${res.status}.` };
    }

    if (id === "custom") {
      if (!baseUrl) return { success: false, message: "Custom Base URL is required." };
      const res = await fetch(baseUrl.replace(/\/$/, ""), {
        headers: apiKey.trim() ? { Authorization: `Bearer ${apiKey.trim()}` } : {},
      });
      return {
        success: res.ok,
        message: res.ok
          ? "Custom endpoint reached successfully!"
          : `Endpoint responded with status HTTP ${res.status}`,
      };
    }

    return { success: true, message: "Configuration format valid." };
  } catch (error: any) {
    return {
      success: false,
      message: error?.message || "Network error while connecting to provider endpoint.",
    };
  }
}

/**
 * Transcribes a recorded Audio Blob using the configured speech-to-text provider.
 */
export async function transcribeAudioBlob(
  blob: Blob,
  settings: AudioSettings,
): Promise<string> {
  const providerId = settings.activeSttProvider;
  const config = settings.providers[providerId];

  if (!config) {
    throw new Error(`Audio provider "${providerId}" is not configured.`);
  }

  // 1. Deepgram STT
  if (providerId === "deepgram") {
    if (!config.apiKey.trim()) {
      throw new Error("Deepgram API Key is required. Please add it in Settings → Audio & Voice.");
    }
    const model = config.model || "nova-3";
    const languageParam = settings.language && settings.language !== "auto" ? `&language=${settings.language}` : "";
    const smartFormat = settings.autoPunctuation ? "&smart_format=true" : "";
    const url = `https://api.deepgram.com/v1/listen?model=${encodeURIComponent(model)}${smartFormat}${languageParam}`;

    const response = await fetch(url, {
      method: "POST",
      headers: {
        Authorization: `Token ${config.apiKey.trim()}`,
        "Content-Type": blob.type || "audio/webm",
      },
      body: blob,
    });

    if (!response.ok) {
      const errText = await response.text().catch(() => "");
      throw new Error(`Deepgram API error (${response.status}): ${errText || response.statusText}`);
    }

    const data = await response.json();
    const transcript =
      data.results?.channels?.[0]?.alternatives?.[0]?.transcript || "";
    return transcript.trim();
  }

  // 2. OpenAI Audio / Whisper
  if (providerId === "openai") {
    if (!config.apiKey.trim()) {
      throw new Error("OpenAI API Key is required. Please add it in Settings → Audio & Voice.");
    }
    const baseUrl = (config.baseUrl || "https://api.openai.com/v1").replace(/\/$/, "");
    const formData = new FormData();
    const ext = blob.type.includes("wav") ? "wav" : blob.type.includes("mp4") ? "m4a" : "webm";
    formData.append("file", blob, `audio.${ext}`);
    formData.append("model", config.model || "whisper-1");
    if (settings.language && settings.language !== "auto") {
      formData.append("language", settings.language);
    }

    const response = await fetch(`${baseUrl}/audio/transcriptions`, {
      method: "POST",
      headers: {
        Authorization: `Bearer ${config.apiKey.trim()}`,
      },
      body: formData,
    });

    if (!response.ok) {
      const errData = await response.json().catch(() => null);
      throw new Error(errData?.error?.message || `OpenAI Whisper error (${response.status})`);
    }

    const data = await response.json();
    return (data.text || "").trim();
  }

  // 3. Groq Whisper
  if (providerId === "groq") {
    if (!config.apiKey.trim()) {
      throw new Error("Groq API Key is required. Please add it in Settings → Audio & Voice.");
    }
    const baseUrl = (config.baseUrl || "https://api.groq.com/openai/v1").replace(/\/$/, "");
    const formData = new FormData();
    const ext = blob.type.includes("wav") ? "wav" : "webm";
    formData.append("file", blob, `audio.${ext}`);
    formData.append("model", config.model || "whisper-large-v3-turbo");
    if (settings.language && settings.language !== "auto") {
      formData.append("language", settings.language);
    }

    const response = await fetch(`${baseUrl}/audio/transcriptions`, {
      method: "POST",
      headers: {
        Authorization: `Bearer ${config.apiKey.trim()}`,
      },
      body: formData,
    });

    if (!response.ok) {
      const errData = await response.json().catch(() => null);
      throw new Error(errData?.error?.message || `Groq Whisper error (${response.status})`);
    }

    const data = await response.json();
    return (data.text || "").trim();
  }

  // 4. Custom Endpoint
  if (providerId === "custom") {
    const baseUrl = (config.baseUrl || "").replace(/\/$/, "");
    if (!baseUrl) {
      throw new Error("Custom audio endpoint Base URL is required.");
    }
    const formData = new FormData();
    formData.append("file", blob, "audio.webm");
    formData.append("model", config.model || "whisper");

    const headers: Record<string, string> = {};
    if (config.apiKey.trim()) {
      headers["Authorization"] = `Bearer ${config.apiKey.trim()}`;
    }

    const targetUrl = baseUrl.endsWith("/transcriptions") ? baseUrl : `${baseUrl}/audio/transcriptions`;
    const response = await fetch(targetUrl, {
      method: "POST",
      headers,
      body: formData,
    });

    if (!response.ok) {
      throw new Error(`Custom endpoint error (${response.status})`);
    }

    const data = await response.json();
    return (data.text || data.transcript || "").trim();
  }

  throw new Error(`Audio provider "${providerId}" does not support direct blob transcription.`);
}
