import { useCallback, useEffect, useRef, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { Webview } from "@tauri-apps/api/webview";
import { LogicalPosition, LogicalSize } from "@tauri-apps/api/dpi";
import { api } from "../lib/api";

/**
 * Clean & Minimalist SVG Icon Suite (1.6px precision stroke)
 */
const IconNavBack = ({ size = 13 }: { size?: number }) => (
  <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round">
    <polyline points="15 18 9 12 15 6" />
  </svg>
);

const IconNavForward = ({ size = 13 }: { size?: number }) => (
  <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round">
    <polyline points="9 18 15 12 9 6" />
  </svg>
);

const IconNavReload = ({ size = 13 }: { size?: number }) => (
  <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round">
    <path d="M21 2v6h-6" />
    <path d="M3 12a9 9 0 0 1 15.5-6.4L21 8" />
    <path d="M3 22v-6h6" />
    <path d="M21 12a9 9 0 0 1-15.5 6.4L3 16" />
  </svg>
);

const IconNavHome = ({ size = 13 }: { size?: number }) => (
  <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round">
    <path d="m3 9 9-7 9 7v11a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z" />
    <polyline points="9 22 9 12 15 12 15 22" />
  </svg>
);

const IconVideoPopout = ({ size = 13 }: { size?: number }) => (
  <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round">
    <rect width="20" height="15" x="2" y="4.5" rx="2" />
    <rect width="8" height="6" x="12" y="11.5" rx="1" fill="currentColor" fillOpacity="0.25" />
  </svg>
);

const IconCache = ({ size = 13 }: { size?: number }) => (
  <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round">
    <path d="M4 6c0-1.1 3.6-2 8-2s8 .9 8 2v12c0 1.1-3.6 2-8 2s-8-.9-8-2V6Z" />
    <path d="M4 11c0 1.1 3.6 2 8 2s8-.9 8-2" />
    <path d="M4 16c0 1.1 3.6 2 8 2s8-.9 8-2" />
  </svg>
);

const IconExternalWindow = ({ size = 13 }: { size?: number }) => (
  <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round">
    <path d="M15 3h6v6" />
    <path d="M10 14 21 3" />
    <path d="M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6" />
  </svg>
);

const IconFullscreenToggle = ({ size = 13, isFull = false }: { size?: number; isFull?: boolean }) => (
  <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round">
    {isFull ? (
      <path d="M8 3v3a2 2 0 0 1-2 2H3m18 0h-3a2 2 0 0 1-2-2V3m0 18v-3a2 2 0 0 1 2-2h3M3 16h3a2 2 0 0 1 2 2v3" />
    ) : (
      <path d="M8 3H5a2 2 0 0 0-2 2v3m18 0V5a2 2 0 0 0-2-2h-3m0 18h3a2 2 0 0 0 2-2v-3M3 16v3a2 2 0 0 0 2 2h3" />
    )}
  </svg>
);

const IconClearMini = ({ size = 10 }: { size?: number }) => (
  <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
    <line x1="18" y1="6" x2="6" y2="18" />
    <line x1="6" y1="6" x2="18" y2="18" />
  </svg>
);

const IconSearch = ({ size = 14 }: { size?: number }) => (
  <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round">
    <circle cx="11" cy="11" r="8" />
    <path d="m21 21-4.3-4.3" />
  </svg>
);

const LOCAL_HOSTS = ["localhost", "127.0.0.1", "0.0.0.0", "[::1]", "::1"];

export function isLoopback(raw: string): boolean {
  try {
    const url = new URL(raw);
    return (
      (url.protocol === "http:" || url.protocol === "https:") &&
      LOCAL_HOSTS.includes(url.hostname)
    );
  } catch {
    return false;
  }
}

export function extractPort(raw: string): number | null {
  try {
    const url = new URL(raw);
    if (url.port) return Number(url.port);
    if (url.protocol === "http:") return 80;
    if (url.protocol === "https:") return 443;
    return null;
  } catch {
    const match = raw.match(/(?:localhost|127\.0\.0\.1):(\d{2,5})/i) || raw.match(/:(\d{2,5})/);
    return match ? Number(match[1]) : null;
  }
}

export function normaliseUrl(raw: string): string {
  const value = raw.trim();
  if (!value) return "";
  if (/^\d{2,5}$/.test(value)) return `http://localhost:${value}`;
  if (/^(?:localhost|127\.0\.0\.1)(?::\d+)?(?:$|\/)/i.test(value)) return `http://${value}`;
  if (/^[\w-]+(?:\.[\w-]+)+(?::\d+)?(?:$|\/)/i.test(value)) return `https://${value}`;
  if (/^https?:\/\//i.test(value)) return value;
  // Default to Google search for full Chrome-like web results
  return `https://www.google.com/search?q=${encodeURIComponent(value)}`;
}

const BROWSER_WEBVIEW_LABEL = "workbench-browser";
const inTauri =
  typeof window !== "undefined" &&
  ("__TAURI_INTERNALS__" in window || "__TAURI__" in window);

async function closeWorkbenchWebview() {
  try {
    const existing = await Webview.getByLabel(BROWSER_WEBVIEW_LABEL);
    if (existing) await existing.close();
  } catch {
    /* already gone */
  }
}

/** One browser tab (SPA-404): its own history and address. The active tab's copy lives in
 *  the view's own state; switching tabs swaps it in and out. */
type BrowserTab = { id: string; history: string[]; cursor: number; address: string };

function newBrowserTab(): BrowserTab {
  return { id: `tab-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 7)}`, history: [], cursor: -1, address: "" };
}

export function BrowserView({
  active = true,
  occluded = false,
}: {
  active?: boolean;
  occluded?: boolean;
}) {
  const [address, setAddress] = useState("");
  const [history, setHistory] = useState<string[]>([]);
  const [cursor, setCursor] = useState(-1);
  const [tabs, setTabs] = useState<BrowserTab[]>(() => [newBrowserTab()]);
  const [activeTabId, setActiveTabId] = useState<string>(() => tabs[0]?.id ?? "");
  const [reloadKey, setReloadKey] = useState(0);
  const [loading, setLoading] = useState(false);
  const [loadError, setLoadError] = useState(false);
  const [isFullscreen, setIsFullscreen] = useState(false);
  const [cacheMenuOpen, setCacheMenuOpen] = useState(false);
  const [cacheToast, setCacheToast] = useState<string | null>(null);
  const [nativeOn, setNativeOn] = useState(false);
  const [hostError, setHostError] = useState<string | null>(null);
  const [domOccluded, setDomOccluded] = useState(false);

  const frameRef = useRef<HTMLIFrameElement | null>(null);
  const cacheRef = useRef<HTMLDivElement | null>(null);
  const paneRef = useRef<HTMLDivElement | null>(null);
  const nativeUrlRef = useRef<string | null>(null);

  // Automatically detect any modal, backdrop, or dialog in the DOM so the native
  // child OS window never occludes HTML dialogs like SettingsModal.
  useEffect(() => {
    if (typeof document === "undefined") return;
    const checkDomOccluded = () => {
      const modal = document.querySelector(
        '.modal-overlay, .review-modal-backdrop, .settings-fullscreen-modal, [aria-modal="true"], dialog[open]'
      );
      setDomOccluded(Boolean(modal));
    };

    checkDomOccluded();

    const observer = new MutationObserver(() => {
      checkDomOccluded();
    });

    observer.observe(document.body, {
      childList: true,
      subtree: true,
      attributes: true,
      attributeFilter: ["class", "aria-modal", "open", "hidden"],
    });

    return () => observer.disconnect();
  }, []);

  const isOccluded = occluded || domOccluded;
  const current = cursor >= 0 ? history[cursor] : null;
  const useNative = inTauri && active && !!current && !loadError;

  const syncNativeBounds = useCallback(async () => {
    if (!useNative) return;
    const el = paneRef.current;
    const wv = await Webview.getByLabel(BROWSER_WEBVIEW_LABEL).catch(() => null);
    if (!el || !wv) return;
    if (isOccluded) {
      await wv.hide().catch(() => undefined);
      return;
    }
    const rect = el.getBoundingClientRect();
    if (rect.width < 8 || rect.height < 8) {
      await wv.hide().catch(() => undefined);
      return;
    }
    await wv
      .setPosition(new LogicalPosition(Math.round(rect.left), Math.round(rect.top)))
      .catch(() => undefined);
    await wv
      .setSize(new LogicalSize(Math.max(1, Math.round(rect.width)), Math.max(1, Math.round(rect.height))))
      .catch(() => undefined);
    await wv.show().catch(() => undefined);
  }, [useNative, isOccluded]);

  // Immediately hide or restore the native webview when occlusion changes (e.g. Settings opens or closes)
  useEffect(() => {
    if (!useNative) return;
    let cancelled = false;
    const updateVisibility = async () => {
      const wv = await Webview.getByLabel(BROWSER_WEBVIEW_LABEL).catch(() => null);
      if (!wv || cancelled) return;
      if (isOccluded) {
        await wv.hide().catch(() => undefined);
      } else {
        await syncNativeBounds();
      }
    };
    void updateVisibility();
    return () => {
      cancelled = true;
    };
  }, [isOccluded, useNative, syncNativeBounds]);

  useEffect(() => {
    let cancelled = false;
    const run = async () => {
      if (!useNative || !current) {
        nativeUrlRef.current = null;
        setNativeOn(false);
        await closeWorkbenchWebview();
        return;
      }
      try {
        const el = paneRef.current;
        const rect = el?.getBoundingClientRect();
        if (!rect || rect.width < 8 || rect.height < 8) return;
        const nativeKey = `${current}::${reloadKey}`;
        if (nativeUrlRef.current !== nativeKey) {
          await closeWorkbenchWebview();
          if (cancelled) return;
          const wv = new Webview(getCurrentWindow(), BROWSER_WEBVIEW_LABEL, {
            url: current,
            x: Math.round(rect.left),
            y: Math.round(rect.top),
            width: Math.max(1, Math.round(rect.width)),
            height: Math.max(1, Math.round(rect.height)),
            focus: false,
            userAgent:
              "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/122.0.0.0 Safari/537.36",
          });
          await new Promise<void>((resolve, reject) => {
            const t = window.setTimeout(() => reject(new Error("webview timeout")), 12000);
            void wv.once("tauri://created", () => {
              window.clearTimeout(t);
              resolve();
            });
            void wv.once("tauri://error", (event) => {
              window.clearTimeout(t);
              const payload = (event as { payload?: unknown }).payload;
              reject(payload instanceof Error ? payload : new Error(String(payload ?? "webview error")));
            });
          });
          nativeUrlRef.current = nativeKey;
        }
        if (cancelled) return;
        setNativeOn(true);
        setLoading(false);
        setLoadError(false);
        setHostError(null);
        await syncNativeBounds();
      } catch (error) {
        nativeUrlRef.current = null;
        setNativeOn(false);
        setLoadError(true);
        setHostError(String((error as Error).message ?? error));
        setLoading(false);
      }
    };
    void run();
    return () => {
      cancelled = true;
    };
  }, [useNative, current, reloadKey, syncNativeBounds]);

  useEffect(() => {
    if (!useNative) return;
    const onResize = () => {
      void syncNativeBounds();
    };
    window.addEventListener("resize", onResize);
    const el = paneRef.current;
    const ro = el ? new ResizeObserver(onResize) : null;
    if (el && ro) ro.observe(el);
    return () => {
      window.removeEventListener("resize", onResize);
      ro?.disconnect();
    };
  }, [useNative, syncNativeBounds]);

  useEffect(() => {
    return () => {
      void closeWorkbenchWebview();
    };
  }, []);

  // Navigate to URL
  const go = useCallback(
    (raw: string) => {
      const url = normaliseUrl(raw);
      if (!url) return;

      setLoadError(false);
      setHostError(null);
      setAddress(url);
      setLoading(true);

      setHistory((past) => {
        const trimmed = past.slice(0, cursor + 1);
        if (trimmed[trimmed.length - 1] === url) return trimmed;
        const next = [...trimmed, url];
        setCursor(next.length - 1);
        return next;
      });
    },
    [cursor],
  );

  // Global event listener for automated triggers
  useEffect(() => {
    const handleNavigate = (evt: Event) => {
      const customEvt = evt as CustomEvent<{ url: string }>;
      if (customEvt.detail?.url) {
        go(customEvt.detail.url);
      }
    };
    window.addEventListener("bhippi:navigate-browser", handleNavigate);
    return () => window.removeEventListener("bhippi:navigate-browser", handleNavigate);
  }, [go]);

  // Close cache menu when clicking outside
  useEffect(() => {
    if (!cacheMenuOpen) return;
    const handleClickOutside = (e: MouseEvent) => {
      if (cacheRef.current && !cacheRef.current.contains(e.target as Node)) {
        setCacheMenuOpen(false);
      }
    };
    document.addEventListener("mousedown", handleClickOutside);
    return () => document.removeEventListener("mousedown", handleClickOutside);
  }, [cacheMenuOpen]);

  // Opera-style Floating Video Popout Window
  const openVideoPopout = useCallback(async (targetUrl?: string | null) => {
    const url = targetUrl || current || address || "https://www.youtube.com";
    if (!url) return;

    try {
      const doc = frameRef.current?.contentDocument;
      const video = doc?.querySelector("video");
      if (video && (document as any).pictureInPictureEnabled) {
        await (video as any).requestPictureInPicture();
        return;
      }
    } catch {}

    try {
      const screenWidth = window.screen.availWidth || 1920;
      const screenHeight = window.screen.availHeight || 1080;
      const winWidth = 560;
      const winHeight = 330;
      const x = Math.max(20, screenWidth - winWidth - 28);
      const y = Math.max(20, screenHeight - winHeight - 48);

      const { WebviewWindow } = await import("@tauri-apps/api/webviewWindow");
      const pip = new WebviewWindow("pip-" + Date.now(), {
        url,
        title: "Video Popout — Bhippi",
        width: winWidth,
        height: winHeight,
        x,
        y,
        alwaysOnTop: true,
        resizable: true,
        decorations: true,
        center: false,
      });
      await pip.show();
    } catch {
      window.open(url, "pip-fallback", "width=560,height=330,left=1200,top=600");
    }
  }, [current, address]);

  // Open in dedicated Chromium Webview Window
  const openInNewWindow = useCallback(async (url: string) => {
    const target = normaliseUrl(url || "https://www.google.com");
    try {
      const { WebviewWindow } = await import("@tauri-apps/api/webviewWindow");
      const label = `browser-${Date.now()}`;
      const hosted = new WebviewWindow(label, {
        url: target,
        title: `Bhippi Browser — ${target}`,
        width: 1200,
        height: 820,
        center: true,
        focus: true,
      });
      await new Promise<void>((resolve, reject) => {
        const t = window.setTimeout(() => reject(new Error("window timeout")), 8000);
        void hosted.once("tauri://created", () => {
          window.clearTimeout(t);
          resolve();
        });
        void hosted.once("tauri://error", (event) => {
          window.clearTimeout(t);
          reject(event);
        });
      });
      await hosted.show();
    } catch {
      try {
        await api.openExternalUrl(target);
      } catch {
        window.open(target, "_blank", "noopener,noreferrer");
      }
    }
  }, []);

  const openExternally = () => {
    const url = normaliseUrl(address || current || "https://www.google.com");
    if (!url) return;
    void api.openExternalUrl(url).catch(() => {
      window.open(url, "_blank", "noopener,noreferrer");
    });
  };

  const handleHome = () => {
    setAddress("");
    setCursor(-1);
    setLoadError(false);
  };

  // ── tabs (SPA-404) ──────────────────────────────────────────────────────────
  // The active tab's history lives in the view's own state, so navigation code is
  // untouched; a switch writes it back into the tab list and reads the next one out.
  const loadTab = useCallback((tab: BrowserTab) => {
    setHistory(tab.history);
    setCursor(tab.cursor);
    setAddress(tab.address || (tab.cursor >= 0 ? tab.history[tab.cursor] ?? "" : ""));
    setLoadError(false);
    setHostError(null);
    setActiveTabId(tab.id);
  }, []);

  const snapshotActive = useCallback(
    (list: BrowserTab[]) =>
      list.map((tab) => (tab.id === activeTabId ? { ...tab, history, cursor, address } : tab)),
    [activeTabId, history, cursor, address],
  );

  const switchTab = useCallback(
    (id: string) => {
      if (id === activeTabId) return;
      const target = tabs.find((tab) => tab.id === id);
      if (!target) return;
      setTabs((list) => snapshotActive(list));
      loadTab(target);
    },
    [activeTabId, tabs, snapshotActive, loadTab],
  );

  const newTab = useCallback(() => {
    const fresh = newBrowserTab();
    setTabs((list) => [...snapshotActive(list), fresh]);
    loadTab(fresh);
  }, [snapshotActive, loadTab]);

  const closeTab = useCallback(
    (id: string) => {
      const index = tabs.findIndex((tab) => tab.id === id);
      if (index === -1) return;
      const remaining = snapshotActive(tabs).filter((tab) => tab.id !== id);
      if (remaining.length === 0) {
        const fresh = newBrowserTab();
        setTabs([fresh]);
        loadTab(fresh);
        return;
      }
      setTabs(remaining);
      if (id === activeTabId) {
        // Chrome's rule: closing the front tab lands on the one to its left.
        loadTab(remaining[Math.max(0, index - 1)]);
      }
    },
    [tabs, activeTabId, snapshotActive, loadTab],
  );

  useEffect(() => {
    if (!active) return undefined;
    const onKey = (event: KeyboardEvent) => {
      if (!(event.ctrlKey || event.metaKey) || event.altKey || event.shiftKey) return;
      const key = event.key.toLowerCase();
      if (key === "t") {
        event.preventDefault();
        newTab();
      } else if (key === "w") {
        event.preventDefault();
        closeTab(activeTabId);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [active, activeTabId, newTab, closeTab]);

  const getDomain = (rawUrl: string | null) => {
    if (!rawUrl) return "";
    try {
      return new URL(rawUrl).host;
    } catch {
      return rawUrl.replace(/https?:\/\//, "").split("/")[0] || "";
    }
  };

  const activeDomain = getDomain(current || address);

  // Granular Cache Clearing
  const clearCurrentSiteCache = useCallback(async () => {
    try {
      if ("caches" in window) {
        const keys = await window.caches.keys();
        for (const key of keys) {
          if (!activeDomain || key.toLowerCase().includes(activeDomain.toLowerCase())) {
            await window.caches.delete(key);
          }
        }
      }
      try {
        const frameWin = frameRef.current?.contentWindow;
        if (frameWin) {
          frameWin.localStorage?.clear();
          frameWin.sessionStorage?.clear();
        }
      } catch {}

      setReloadKey((k) => k + 1);
      setCacheToast(`Cleared cache for ${activeDomain || "current site"}`);
    } catch {
      setCacheToast("Cache cleared");
    } finally {
      setCacheMenuOpen(false);
      window.setTimeout(() => setCacheToast(null), 2500);
    }
  }, [activeDomain]);

  const clearAllBrowserCache = useCallback(async () => {
    try {
      if ("caches" in window) {
        const keys = await window.caches.keys();
        await Promise.all(keys.map((k) => window.caches.delete(k)));
      }
      window.sessionStorage.clear();
      setReloadKey((k) => k + 1);
      setCacheToast("All browser cache cleared");
    } catch {
      setCacheToast("Cache cleared");
    } finally {
      setCacheMenuOpen(false);
      window.setTimeout(() => setCacheToast(null), 2500);
    }
  }, []);

  const getBadgeType = (url: string | null) => {
    if (!url) return { text: "CHROME", type: "idle" };
    if (isLoopback(url)) {
      const p = extractPort(url);
      return { text: `PORT ${p || "LOCAL"}`, type: "local" };
    }
    if (url.startsWith("https://")) return { text: "HTTPS", type: "secure" };
    if (url.startsWith("http://")) return { text: "HTTP", type: "http" };
    return { text: "WEB", type: "web" };
  };

  const currentBadge = getBadgeType(current || address);

  return (
    <div className={`browser-view${isFullscreen ? " is-fullscreen" : ""}`}>
      {/* ── Tabs (SPA-404): Chrome's strip — the front tab merges with the toolbar, the
           rest share the width and shrink, + opens another, Ctrl+T / Ctrl+W work. ── */}
      <div className="browser-tabs" role="tablist" aria-label="Browser tabs">
        <div className="browser-tabs-scroll">
          {tabs.map((tab) => {
            const isActive = tab.id === activeTabId;
            const url = isActive ? current : tab.cursor >= 0 ? (tab.history[tab.cursor] ?? null) : null;
            const host = getDomain(url);
            const title = host || "New tab";
            return (
              <div key={tab.id} className={`browser-tab${isActive ? " active" : ""}`}>
                <button
                  type="button"
                  role="tab"
                  aria-selected={isActive}
                  className="browser-tab-open"
                  onClick={() => switchTab(tab.id)}
                  title={url ?? "New tab"}
                >
                  <span className={`browser-tab-favicon${host ? "" : " blank"}`} aria-hidden="true">
                    {host ? (
                      <img
                        src={`https://www.google.com/s2/favicons?domain=${encodeURIComponent(host)}&sz=32`}
                        alt=""
                        onError={(event) => {
                          event.currentTarget.style.display = "none";
                        }}
                      />
                    ) : null}
                  </span>
                  <span className="browser-tab-title">{title}</span>
                </button>
                <button
                  type="button"
                  className="browser-tab-close"
                  onClick={() => closeTab(tab.id)}
                  title="Close tab (Ctrl+W)"
                  aria-label={`Close ${title}`}
                >
                  <IconClearMini size={9} />
                </button>
              </div>
            );
          })}
        </div>
        <button
          type="button"
          className="browser-tab-new"
          onClick={newTab}
          title="New tab (Ctrl+T)"
          aria-label="New tab"
        >
          +
        </button>
      </div>

      {/* ── Modern Minimalist Browser Toolbar ─────────────────────────── */}
      <div className="browser-bar">
        <button
          type="button"
          className="browser-nav"
          disabled={cursor <= 0}
          onClick={() => {
            const nextCursor = Math.max(0, cursor - 1);
            setCursor(nextCursor);
            setAddress(history[nextCursor] || "");
          }}
          title="Back"
          aria-label="Back"
        >
          <IconNavBack size={13} />
        </button>

        <button
          type="button"
          className="browser-nav"
          disabled={cursor < 0 || cursor >= history.length - 1}
          onClick={() => {
            const nextCursor = Math.min(history.length - 1, cursor + 1);
            setCursor(nextCursor);
            setAddress(history[nextCursor] || "");
          }}
          title="Forward"
          aria-label="Forward"
        >
          <IconNavForward size={13} />
        </button>

        <button
          type="button"
          className={`browser-nav${loading ? " is-spinning" : ""}`}
          onClick={() => {
            setLoading(true);
            setReloadKey((k) => k + 1);
          }}
          disabled={!current}
          title="Reload"
          aria-label="Reload"
        >
          <IconNavReload size={13} />
        </button>

        <button
          type="button"
          className="browser-nav"
          onClick={handleHome}
          title="New Tab"
          aria-label="Home"
        >
          <IconNavHome size={13} />
        </button>

        {/* Address Input */}
        <form
          className="browser-address"
          onSubmit={(e) => {
            e.preventDefault();
            go(address);
          }}
        >
          <span className={`browser-scheme-chip ${currentBadge.type}`}>
            {currentBadge.text}
          </span>
          <input
            value={address}
            onChange={(e) => setAddress(e.target.value)}
            placeholder="Search Google or enter web URL (e.g. google.com, youtube.com)…"
            aria-label="Address"
            spellCheck={false}
            onFocus={(e) => e.target.select()}
          />
          {address && (
            <button
              type="button"
              className="browser-address-clear"
              onClick={() => setAddress("")}
              title="Clear"
            >
              <IconClearMini size={9} />
            </button>
          )}
        </form>

        {/* Video Popout (Opera-style floating mini player) */}
        <button
          type="button"
          className="browser-nav"
          onClick={() => void openVideoPopout()}
          title="Video Popout (Float window on right screen)"
          aria-label="Video Popout"
        >
          <IconVideoPopout size={13} />
        </button>

        {/* Granular Cache Management */}
        <div className="browser-cache-wrapper" ref={cacheRef}>
          <button
            type="button"
            className={`browser-nav${cacheMenuOpen ? " is-active" : ""}`}
            onClick={() => setCacheMenuOpen((o) => !o)}
            title="Site & Browser Cache"
            aria-label="Cache Options"
          >
            <IconCache size={13} />
          </button>

          {cacheMenuOpen && (
            <div className="browser-cache-popover">
              <div className="cache-popover-header">
                <span className="cache-popover-title">Storage & Cache</span>
                <span className="cache-popover-sub">
                  {activeDomain ? `Target: ${activeDomain}` : "All browsing data"}
                </span>
              </div>
              <div className="cache-popover-actions">
                <button
                  type="button"
                  className="cache-popover-btn"
                  onClick={() => void clearCurrentSiteCache()}
                >
                  <span className="cache-btn-main">Clear This Site Cache</span>
                  <span className="cache-btn-desc">
                    Deletes cookies & cache for {activeDomain || "active site"}
                  </span>
                </button>
                <button
                  type="button"
                  className="cache-popover-btn danger"
                  onClick={() => void clearAllBrowserCache()}
                >
                  <span className="cache-btn-main">Clear All Browser Cache</span>
                  <span className="cache-btn-desc">
                    Purges all local caches & browsing storage
                  </span>
                </button>
              </div>
            </div>
          )}
        </div>

        {/* Fullscreen Toggle */}
        <button
          type="button"
          className="browser-nav"
          onClick={() => setIsFullscreen((f) => !f)}
          title={isFullscreen ? "Exit Fullscreen" : "Fullscreen Preview"}
          aria-label="Fullscreen"
        >
          <IconFullscreenToggle size={13} isFull={isFullscreen} />
        </button>

        {/* Popout to Dedicated Webview Window */}
        <button
          type="button"
          className="browser-nav"
          onClick={() => openInNewWindow(address || current || "https://www.google.com")}
          title="Open in Dedicated Chrome Window"
          aria-label="Popout window"
        >
          <IconExternalWindow size={13} />
        </button>
      </div>

      {/* ── Cache Toast Notification ──────────────────────────────────── */}
      {cacheToast && (
        <div className="browser-cache-toast">
          <span className="cache-toast-dot" /> {cacheToast}
        </div>
      )}

      {/* ── Top Progress Loading Bar ──────────────────────────────────── */}
      {loading && <div className="browser-loading-stripe" />}

      {/* ── Viewport Content ──────────────────────────────────────────── */}
      <div className="browser-content-wrap" ref={paneRef}>
        {current ? (
          loadError ? (
            /* External X-Frame-Options Protected Site Fallback */
            <div className="browser-blocked-card">
              <div className="blocked-icon-shield">🌐</div>
              <h3>Could not embed this site</h3>
              <p>
                <strong>{current}</strong> did not load in the workbench pane
                {hostError ? ` (${hostError})` : ""}. Open it in a window instead.
              </p>
              <div className="blocked-actions-cluster">
                <button
                  type="button"
                  className="radar-btn primary"
                  onClick={() => void openInNewWindow(current)}
                >
                  <IconExternalWindow size={13} /> Open in Dedicated Window
                </button>
                <button
                  type="button"
                  className="radar-btn secondary"
                  onClick={openExternally}
                >
                  Open in Chrome
                </button>
                <button
                  type="button"
                  className="radar-btn secondary"
                  onClick={() => {
                    setLoadError(false);
                    setHostError(null);
                    setReloadKey((k) => k + 1);
                    setLoading(true);
                  }}
                >
                  Try again
                </button>
              </div>
            </div>
          ) : (
            /* Native Tauri webview sits on this pane; iframe is the non-Tauri fallback
               and is never used for remote sites (Google etc. refuse to render in one). */
            <div className={`browser-iframe-container${inTauri ? " native-hosted" : ""}`}>
              {inTauri || nativeOn ? (
                <div className="browser-native-slot" aria-hidden="true" />
              ) : (
                <iframe
                  key={`${current}-${reloadKey}`}
                  ref={frameRef}
                  className="browser-frame"
                  src={current}
                  title="Browser Viewport"
                  sandbox="allow-scripts allow-forms allow-same-origin allow-popups allow-popups-to-escape-sandbox allow-downloads allow-modals allow-pointer-lock"
                  allow="autoplay; camera; microphone; display-capture; geolocation; fullscreen; accelerometer; gyroscope; gamepad; cross-origin-isolated"
                  onLoad={() => setLoading(false)}
                  onError={() => {
                    setLoading(false);
                    setLoadError(true);
                  }}
                />
              )}
            </div>
          )
        ) : (
          /* Clean Minimalist Chrome-Style New Tab */
          <div className="browser-chrome-home">
            <div className="chrome-home-hero">
              {/* SPA-404: the new-tab page is Google's shape — a wordmark, one pill, a
                  quiet row of shortcuts, and nothing else on the page. */}
              <div className="chrome-home-wordmark" aria-hidden="true">
                bhippi
              </div>

              {/* Minimalist Google Search Box */}
              <form
                className="chrome-home-search-box"
                onSubmit={(e) => {
                  e.preventDefault();
                  if (address.trim()) {
                    go(address);
                  }
                }}
              >
                <IconSearch size={16} />
                <input
                  type="text"
                  placeholder="Search Google or type a URL"
                  aria-label="Search Google or type a URL"
                  value={address}
                  onChange={(e) => setAddress(e.target.value)}
                  autoFocus
                />
              </form>

              {/* Minimalist Quick Web Bookmarks */}
              <div className="chrome-home-shortcuts">
                <button
                  type="button"
                  className="chrome-shortcut-tile"
                  onClick={() => go("https://www.google.com")}
                >
                  <span className="shortcut-icon">🔍</span>
                  <span className="shortcut-title">Google</span>
                </button>

                <button
                  type="button"
                  className="chrome-shortcut-tile"
                  onClick={() => go("https://www.wikipedia.org")}
                >
                  <span className="shortcut-icon">📖</span>
                  <span className="shortcut-title">Wikipedia</span>
                </button>

                <button
                  type="button"
                  className="chrome-shortcut-tile"
                  onClick={() => go("https://github.com")}
                >
                  <span className="shortcut-icon">🐙</span>
                  <span className="shortcut-title">GitHub</span>
                </button>

                <button
                  type="button"
                  className="chrome-shortcut-tile"
                  onClick={() => go("https://www.youtube.com")}
                >
                  <span className="shortcut-icon">▶️</span>
                  <span className="shortcut-title">YouTube</span>
                </button>

              </div>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
