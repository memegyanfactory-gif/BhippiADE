// The Add-ons gallery (GAD-008; docs/04-PAGES.md, ADR-0028). Everything this screen shows about
// a plugin — its badge, its one primary button, whether it can be switched on — is
// decided in `bhippi-app/src/plugins.rs` and arrives on `PluginMetadata`. What lives
// here is presentation only: search, the tab, the category, the sort order (INV-032).

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { ComponentType, CSSProperties } from "react";
import type { PluginAction, PluginMetadata, PluginStatus } from "../lib/ipc";
import { api } from "../lib/api";
import {
  IconArrowUp,
  IconBox,
  IconBrain,
  IconBrowser,
  IconClose,
  IconDownload,
  IconExternalLink,
  IconFolder,
  IconGauge,
  IconGitBranch,
  IconRefresh,
  IconSearch,
  IconTerminal,
  IconTrash,
} from "../components/icons";

/// A card's glyph, keyed by the catalogue's `icon`. An unknown key falls back rather
/// than rendering a hole.
const GLYPHS: Record<string, ComponentType<{ size?: number }>> = {
  browser: IconBrowser,
  terminal: IconTerminal,
  git: IconGitBranch,
  website: IconExternalLink,
  memory: IconBrain,
  deployment: IconArrowUp,
  analytics: IconGauge,
  assets: IconFolder,
};

/// The icon tile's hue. Decorative only — every card states its status in words, so
/// nothing here is the sole carrier of meaning (INV-034).
const TINTS: Record<string, string> = {
  browser: "#5b9dd9",
  terminal: "#b9b2a6",
  git: "#e8703a",
  website: "#5b9dd9",
  memory: "#a781e0",
  deployment: "#4f9de0",
  analytics: "#e8963a",
  assets: "#4fb06a",
};

const STATUS_LABEL: Record<PluginStatus, string> = {
  built_in: "Built-in",
  installed: "Installed",
  update_available: "Update Available",
  needs_setup: "Needs Setup",
  beta: "Beta",
  available: "Available",
};

const ACTION_LABEL: Record<PluginAction, string> = {
  open: "Open",
  install: "Install",
  update: "Update",
  configure: "Configure",
};

const ACTION_PENDING: Record<PluginAction, string> = {
  open: "Opening…",
  install: "Installing…",
  update: "Updating…",
  configure: "Opening…",
};

const TABS = [
  { id: "all", label: "All" },
  { id: "installed", label: "Installed" },
  { id: "available", label: "Available" },
  { id: "builtin", label: "Built-in" },
  { id: "updates", label: "Updates" },
] as const;

type Tab = (typeof TABS)[number]["id"];
type Sort = "recent" | "name" | "category";

function inTab(plugin: PluginMetadata, tab: Tab): boolean {
  switch (tab) {
    case "installed":
      return plugin.installed;
    case "available":
      return !plugin.installed;
    case "builtin":
      return plugin.built_in;
    case "updates":
      return plugin.status === "update_available";
    default:
      return true;
  }
}

/// IPC errors arrive as `{ message, hint }`; anything else is stringified rather than
/// swallowed, so a failure is never silent.
function describe(error: unknown): string {
  if (error && typeof error === "object" && "message" in error) {
    const { message, hint } = error as { message?: string; hint?: string | null };
    return [message, hint].filter(Boolean).join(" — ");
  }
  return String((error as Error)?.message ?? error);
}

export function AddOns({ onOpenTarget }: { onOpenTarget: (target: string) => void }) {
  const [plugins, setPlugins] = useState<PluginMetadata[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);

  const [query, setQuery] = useState("");
  const [category, setCategory] = useState("all");
  const [sort, setSort] = useState<Sort>("recent");
  const [installedOnly, setInstalledOnly] = useState(false);
  const [tab, setTab] = useState<Tab>("all");

  const [busy, setBusy] = useState<string | null>(null);
  const [menuFor, setMenuFor] = useState<string | null>(null);
  const [installOpen, setInstallOpen] = useState(false);
  const [installRef, setInstallRef] = useState("");
  const installInput = useRef<HTMLInputElement>(null);

  const load = useCallback(async () => {
    try {
      setPlugins(await api.listPlugins());
      setError(null);
    } catch (loadError) {
      setError(describe(loadError));
    }
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  useEffect(() => {
    if (installOpen) installInput.current?.focus();
  }, [installOpen]);

  // One open menu at a time, and Escape or a click anywhere else closes it.
  useEffect(() => {
    if (menuFor === null) return undefined;
    const dismiss = () => setMenuFor(null);
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") setMenuFor(null);
    };
    window.addEventListener("click", dismiss);
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("click", dismiss);
      window.removeEventListener("keydown", onKey);
    };
  }, [menuFor]);

  /// Runs one mutation, then re-reads the list so the card shows what Rust now believes
  /// rather than what the click hoped for.
  const run = useCallback(
    async (plugin: PluginMetadata, work: () => Promise<unknown>, done: string) => {
      setBusy(plugin.id);
      setNotice(null);
      try {
        await work();
        await load();
        setNotice(done);
      } catch (actionError) {
        setError(describe(actionError));
      } finally {
        setBusy(null);
      }
    },
    [load],
  );

  const primary = useCallback(
    (plugin: PluginMetadata) => {
      switch (plugin.action) {
        case "install":
          return run(plugin, () => api.installPlugin(plugin.id), `${plugin.name} installed.`);
        case "update":
          return run(plugin, () => api.updatePlugin(plugin.id), `${plugin.name} updated.`);
        case "configure":
          if (plugin.settings_tab) onOpenTarget(`settings:${plugin.settings_tab}`);
          return Promise.resolve();
        default:
          if (plugin.target) onOpenTarget(plugin.target);
          return Promise.resolve();
      }
    },
    [onOpenTarget, run],
  );

  const categories = useMemo(() => {
    const seen = new Set((plugins ?? []).map((plugin) => plugin.category));
    return [...seen].sort((a, b) => a.localeCompare(b));
  }, [plugins]);

  const shown = useMemo(() => {
    const needle = query.trim().toLowerCase();
    const filtered = (plugins ?? []).filter((plugin) => {
      if (!inTab(plugin, tab)) return false;
      if (installedOnly && !plugin.installed) return false;
      if (category !== "all" && plugin.category !== category) return false;
      if (needle.length === 0) return true;
      return (
        plugin.name.toLowerCase().includes(needle) ||
        plugin.description.toLowerCase().includes(needle) ||
        plugin.category.toLowerCase().includes(needle)
      );
    });
    const ordered = [...filtered];
    if (sort === "name") ordered.sort((a, b) => a.name.localeCompare(b.name));
    if (sort === "category")
      ordered.sort(
        (a, b) => a.category.localeCompare(b.category) || a.name.localeCompare(b.name),
      );
    // "Recent" keeps the catalogue's own order and floats anything the user installed
    // above it, newest first.
    if (sort === "recent") ordered.sort((a, b) => b.installed_at - a.installed_at);
    return ordered;
  }, [plugins, tab, installedOnly, category, query, sort]);

  const updates = (plugins ?? []).filter(
    (plugin) => plugin.status === "update_available",
  ).length;
  const filtering = query.trim().length > 0 || category !== "all" || installedOnly;

  const install = async () => {
    const reference = installRef.trim();
    if (reference.length === 0) return;
    setBusy(reference);
    setNotice(null);
    try {
      await api.installPlugin(reference);
      await load();
      setNotice(`Installed ${reference}.`);
      setInstallRef("");
      setInstallOpen(false);
    } catch (installError) {
      setError(describe(installError));
    } finally {
      setBusy(null);
    }
  };

  return (
    <div className="pane">
      <div className="pane-inner plugins-inner">
        <header className="plugins-head">
          <div>
            <h1 className="screen-title">Add-ons</h1>
            <p className="screen-sub">
              Browse, install, enable and manage the capabilities your games can use.
            </p>
          </div>

          <div className="plugins-tools">
            <div className="plugins-search">
              <IconSearch size={13} />
              <input
                type="search"
                value={query}
                onChange={(event) => setQuery(event.target.value)}
                placeholder="Search plugins..."
                aria-label="Search plugins"
              />
            </div>

            <select
              className="plugin-select"
              value={category}
              onChange={(event) => setCategory(event.target.value)}
              aria-label="Filter by category"
            >
              <option value="all">All Categories</option>
              {categories.map((name) => (
                <option key={name} value={name}>
                  {name}
                </option>
              ))}
            </select>

            <select
              className="plugin-select"
              value={sort}
              onChange={(event) => setSort(event.target.value as Sort)}
              aria-label="Sort plugins"
            >
              <option value="recent">Sort: Recent</option>
              <option value="name">Sort: Name</option>
              <option value="category">Sort: Category</option>
            </select>

            <label className="plugins-only">
              <span>Installed only</span>
              <Toggle
                checked={installedOnly}
                onChange={setInstalledOnly}
                label="Show installed plugins only"
              />
            </label>
          </div>
        </header>

        <div className="plugins-tabs">
          <div role="tablist" aria-label="Plugin filter">
            {TABS.map(({ id, label }) => (
              <button
                key={id}
                role="tab"
                aria-selected={tab === id}
                className={`plugin-tab${tab === id ? " active" : ""}`}
                onClick={() => setTab(id)}
              >
                {label}
                {id === "updates" && updates > 0 ? (
                  <span className="plugin-tab-count">{updates}</span>
                ) : null}
              </button>
            ))}
          </div>
          <button
            className="plugin-install-open"
            onClick={() => setInstallOpen((open) => !open)}
            aria-expanded={installOpen}
          >
            <IconDownload size={13} />
            Install from URL
          </button>
        </div>

        {installOpen ? (
          <div className="plugin-install-row">
            <input
              ref={installInput}
              value={installRef}
              onChange={(event) => setInstallRef(event.target.value)}
              onKeyDown={(event) => {
                if (event.key === "Enter") void install();
                if (event.key === "Escape") setInstallOpen(false);
              }}
              placeholder="https://example.com/plugin.json — or a catalogue id such as website"
              aria-label="Plugin URL or catalogue id"
            />
            <button
              className="btn-accent"
              onClick={() => void install()}
              disabled={installRef.trim().length === 0 || busy !== null}
            >
              {busy === installRef.trim() ? "Installing…" : "Install"}
            </button>
            <button
              className="btn-primary"
              onClick={() => setInstallOpen(false)}
              aria-label="Cancel install"
            >
              <IconClose size={12} />
            </button>
          </div>
        ) : null}

        {error ? (
          <div className="plugin-banner error" role="alert">
            <span>{error}</span>
            <button className="btn-primary" onClick={() => void load()}>
              Retry
            </button>
          </div>
        ) : null}

        {notice ? (
          <div className="plugin-banner" role="status">
            <span>{notice}</span>
            <button onClick={() => setNotice(null)} aria-label="Dismiss">
              <IconClose size={11} />
            </button>
          </div>
        ) : null}

        {plugins === null && error === null ? (
          <div className="plugin-grid" aria-busy="true">
            {[0, 1, 2, 3, 4, 5].map((slot) => (
              <div key={slot} className="plugin-tile skeleton" />
            ))}
          </div>
        ) : shown.length === 0 ? (
          <div className="plugin-none">
            <p>
              {filtering || tab !== "all"
                ? "No plugins match this filter."
                : "No plugins are available yet."}
            </p>
            {filtering || tab !== "all" ? (
              <button
                className="btn-primary"
                onClick={() => {
                  setQuery("");
                  setCategory("all");
                  setInstalledOnly(false);
                  setTab("all");
                }}
              >
                Clear filters
              </button>
            ) : (
              <button className="btn-accent" onClick={() => setInstallOpen(true)}>
                Install from URL
              </button>
            )}
          </div>
        ) : (
          <div className="plugin-grid">
            {shown.map((plugin) => (
              <Card
                key={plugin.id}
                plugin={plugin}
                busy={busy === plugin.id}
                menuOpen={menuFor === plugin.id}
                onMenu={(open) => setMenuFor(open ? plugin.id : null)}
                onPrimary={() => void primary(plugin)}
                onToggle={(enabled) =>
                  void run(
                    plugin,
                    () =>
                      enabled ? api.activatePlugin(plugin.id) : api.deactivatePlugin(plugin.id),
                    `${plugin.name} ${enabled ? "enabled" : "disabled"}.`,
                  )
                }
                onUninstall={() =>
                  void run(
                    plugin,
                    () => api.uninstallPlugin(plugin.id),
                    `${plugin.name} removed.`,
                  )
                }
                onOpenTarget={onOpenTarget}
              />
            ))}
          </div>
        )}
      </div>
    </div>
  );
}

function Card({
  plugin,
  busy,
  menuOpen,
  onMenu,
  onPrimary,
  onToggle,
  onUninstall,
  onOpenTarget,
}: {
  plugin: PluginMetadata;
  busy: boolean;
  menuOpen: boolean;
  onMenu: (open: boolean) => void;
  onPrimary: () => void;
  onToggle: (enabled: boolean) => void;
  onUninstall: () => void;
  onOpenTarget: (target: string) => void;
}) {
  const Glyph = GLYPHS[plugin.icon] ?? IconBox;
  const tint = TINTS[plugin.icon] ?? "var(--accent)";
  const label = busy ? ACTION_PENDING[plugin.action] : ACTION_LABEL[plugin.action];
  const canUninstall = plugin.installed && !plugin.built_in;

  return (
    <article className={`plugin-tile${plugin.activated ? " on" : ""}`}>
      <div className="plugin-tile-top">
        <span
          className="plugin-glyph"
          style={{ "--plugin-tint": tint } as CSSProperties}
          aria-hidden="true"
        >
          <Glyph size={20} />
        </span>
        <span className={`plugin-badge ${plugin.status}`}>{STATUS_LABEL[plugin.status]}</span>
      </div>

      <h2 className="plugin-name">{plugin.name}</h2>
      <p className="plugin-copy">{plugin.description}</p>

      <div className="plugin-tile-foot">
        <button
          className={plugin.action === "open" ? "btn-primary plugin-go" : "btn-accent plugin-go"}
          onClick={onPrimary}
          disabled={busy}
        >
          {label}
          {plugin.action === "open" ? <IconExternalLink size={11} /> : null}
          {plugin.action === "install" ? <IconDownload size={11} /> : null}
          {plugin.action === "update" ? <IconRefresh size={11} /> : null}
        </button>

        <div className="plugin-tile-right">
          <Toggle
            checked={plugin.activated}
            disabled={busy || !plugin.installed}
            onChange={onToggle}
            label={
              plugin.installed
                ? `Enable ${plugin.name}`
                : `${plugin.name} must be installed before it can be enabled`
            }
          />
          <div className="plugin-menu-wrap" onClick={(event) => event.stopPropagation()}>
            <button
              className="plugin-more"
              aria-haspopup="menu"
              aria-expanded={menuOpen}
              aria-label={`More actions for ${plugin.name}`}
              onClick={() => onMenu(!menuOpen)}
            >
              ⋮
            </button>
            {menuOpen ? (
              <div className="plugin-menu" role="menu">
                <p className="plugin-menu-head">
                  v{plugin.version} · {plugin.category}
                </p>
                {plugin.target && plugin.installed ? (
                  <button
                    role="menuitem"
                    onClick={() => {
                      onMenu(false);
                      if (plugin.target) onOpenTarget(plugin.target);
                    }}
                  >
                    <IconExternalLink size={12} /> Open
                  </button>
                ) : null}
                {plugin.settings_tab ? (
                  <button
                    role="menuitem"
                    onClick={() => {
                      onMenu(false);
                      onOpenTarget(`settings:${plugin.settings_tab}`);
                    }}
                  >
                    <IconGauge size={12} /> Configure
                  </button>
                ) : null}
                {canUninstall ? (
                  <button
                    role="menuitem"
                    className="danger"
                    onClick={() => {
                      onMenu(false);
                      onUninstall();
                    }}
                  >
                    <IconTrash size={12} /> Uninstall
                  </button>
                ) : (
                  <p className="plugin-menu-note">
                    {plugin.built_in
                      ? "Built in — switch it off instead."
                      : "Not installed."}
                  </p>
                )}
              </div>
            ) : null}
          </div>
        </div>
      </div>
    </article>
  );
}

function Toggle({
  checked,
  disabled,
  onChange,
  label,
}: {
  checked: boolean;
  disabled?: boolean;
  onChange: (checked: boolean) => void;
  label: string;
}) {
  return (
    <button
      role="switch"
      aria-checked={checked}
      aria-label={label}
      title={label}
      className={`switch${checked ? " on" : ""}`}
      disabled={disabled}
      onClick={() => onChange(!checked)}
    >
      <span className="knob" />
    </button>
  );
}
