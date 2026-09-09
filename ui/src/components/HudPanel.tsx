// The HUD tab of the Studio dock: pick a HUD, pick a look, build it into the game.
//
// Nothing here decides anything. Rust owns the presets, the skins, the doctrine that
// refuses a bad one, and the build that writes the scene; this file draws that answer and
// sends back two strings. The preview is drawn from the same slot and widget data the
// builder uses, so what the card shows is where the widget actually lands — a mock-up that
// could drift from the build would be worse than no preview.

import { useCallback, useEffect, useMemo, useState } from "react";
import { api } from "../lib/api";
import type {
  FabPack,
  FabVaultView,
  HudApplyResult,
  HudLibraryView,
  HudPresetView,
  HudProjectState,
  HudSkinView,
  HudWidgetView,
  IconImport,
} from "../lib/ipc";

interface HudPanelProps {
  projectPath?: string;
  /** The game's archetype, when the project knows it: it orders the presets. */
  archetype?: string;
  /** Told when a build lands, so the dock can refresh the assets and version lists. */
  onApplied?: (result: HudApplyResult) => void;
}

type Loadable<T> =
  | { state: "idle" }
  | { state: "loading" }
  | { state: "ready"; data: T }
  | { state: "error"; message: string };

const IDLE: Loadable<never> = { state: "idle" };

/** The licence the Fab import pre-fills. The user confirms it; Bhippi never assumes it. */
const SUGGESTED_LICENCE = "Fab Standard License";

function errorText(cause: unknown): string {
  const value = cause as { message?: string; hint?: string } | undefined;
  if (value && typeof value.message === "string") {
    return value.hint ? `${value.message} ${value.hint}` : value.message;
  }
  return String(cause);
}

/** Slot id → the 3×3 cell it occupies in the preview. */
const CELL: Record<string, string> = {
  top_left: "1 / 1",
  top_centre: "1 / 2",
  top_right: "1 / 3",
  mid_left: "2 / 1",
  centre: "2 / 2",
  mid_right: "2 / 3",
  bottom_left: "3 / 1",
  bottom_centre: "3 / 2",
  bottom_right: "3 / 3",
};

const KIND_LABEL: Record<string, string> = {
  bar: "meter",
  segments: "pips",
  counter: "number",
  timer: "clock",
  text: "line",
  reticle: "reticle",
  ring: "dial",
  compass: "compass",
  minimap: "map",
  toast: "notice",
  icon_row: "slots",
  action: "button",
};

/**
 * One preset drawn as the screen it makes: a 3×3 of the anchor slots, each carrying the
 * widgets that land in it. Persistent elements are solid; on-change ones are outlined,
 * because the difference between "always there" and "appears when it matters" is the whole
 * of a HUD's budget.
 */
function HudPreview({ preset, skin }: { preset: HudPresetView; skin: HudSkinView }) {
  const slots = useMemo(() => {
    const grouped = new Map<string, HudWidgetView[]>();
    for (const widget of preset.widgets) {
      const list = grouped.get(widget.slot) ?? [];
      list.push(widget);
      grouped.set(widget.slot, list);
    }
    return grouped;
  }, [preset]);

  return (
    <div className="hud-preview" style={{ borderColor: skin.track }}>
      {Object.entries(CELL).map(([slot, area]) => {
        const widgets = slots.get(slot) ?? [];
        return (
          <div key={slot} className="hud-preview-slot" style={{ gridArea: area }}>
            {widgets.map((widget) => {
              const persistent = widget.visibility === "persistent";
              return (
                <span
                  key={widget.name}
                  className={`hud-chip${persistent ? " persistent" : ""}`}
                  style={{
                    background: persistent ? skin.plate : "transparent",
                    borderColor: persistent ? skin.accent : skin.muted,
                    color: skin.text,
                  }}
                  title={`${widget.name} — ${KIND_LABEL[widget.kind] ?? widget.kind}${
                    widget.binding ? ` · reads ${widget.binding}` : ""
                  }`}
                >
                  {widget.kind === "bar" || widget.kind === "ring" ? (
                    <i className="hud-chip-meter" style={{ background: skin.accent }} />
                  ) : null}
                  {widget.caption || widget.name}
                </span>
              );
            })}
          </div>
        );
      })}
      <span className="hud-preview-safe" style={{ borderColor: skin.muted }} />
    </div>
  );
}

export function HudPanel({ projectPath, archetype = "", onApplied }: HudPanelProps) {
  const [library, setLibrary] = useState<Loadable<HudLibraryView>>(IDLE);
  const [project, setProject] = useState<Loadable<HudProjectState>>(IDLE);
  const [vault, setVault] = useState<Loadable<FabVaultView>>(IDLE);

  const [preset, setPreset] = useState<string>("");
  const [skin, setSkin] = useState<string>("");
  const [busy, setBusy] = useState(false);
  const [notice, setNotice] = useState<string | null>(null);
  const [problem, setProblem] = useState<string | null>(null);

  const [importPack, setImportPack] = useState<FabPack | null>(null);
  const [licence, setLicence] = useState(SUGGESTED_LICENCE);

  const load = useCallback(async () => {
    setLibrary({ state: "loading" });
    try {
      const data = await api.hudLibrary(archetype);
      setLibrary({ state: "ready", data });
      if (!preset && data.presets.length > 0) setPreset(data.presets[0].id);
    } catch (cause) {
      setLibrary({ state: "error", message: errorText(cause) });
    }
  }, [archetype, preset]);

  const loadProject = useCallback(async () => {
    if (!projectPath) return;
    setProject({ state: "loading" });
    try {
      const data = await api.hudProjectState(projectPath);
      setProject({ state: "ready", data });
      // A project that already has a HUD opens on the one it has, not on the first card.
      if (data.preset) setPreset(data.preset);
      if (data.skin) setSkin(data.skin);
    } catch (cause) {
      setProject({ state: "error", message: errorText(cause) });
    }
  }, [projectPath]);

  useEffect(() => {
    void load();
  }, [archetype]); // eslint-disable-line react-hooks/exhaustive-deps

  useEffect(() => {
    void loadProject();
  }, [loadProject]);

  const chosen: HudPresetView | undefined = useMemo(() => {
    if (library.state !== "ready") return undefined;
    return library.data.presets.find((entry) => entry.id === preset) ?? library.data.presets[0];
  }, [library, preset]);

  const chosenSkin: HudSkinView | undefined = useMemo(() => {
    if (library.state !== "ready" || !chosen) return undefined;
    const wanted = skin || chosen.skin;
    return library.data.skins.find((entry) => entry.id === wanted) ?? library.data.skins[0];
  }, [library, chosen, skin]);

  /** Roles this preset wants that the project has no art for. */
  const missingIcons = useMemo(() => {
    if (!chosen) return [];
    const have = project.state === "ready" ? project.data.icons : {};
    return chosen.icon_roles.filter((role) => !(role in have));
  }, [chosen, project]);

  const apply = useCallback(async () => {
    if (!projectPath || !chosen) return;
    setBusy(true);
    setProblem(null);
    setNotice(null);
    try {
      const result = await api.hudApply(projectPath, chosen.id, skin, "user");
      const missing = result.unresolved_icons.length;
      setNotice(
        `${result.replaced ? "Replaced" : "Built"} the ${chosen.title} HUD in ${
          result.skin
        }.${missing > 0 ? ` ${missing} icon${missing === 1 ? "" : "s"} fell back to a caption.` : ""}`,
      );
      onApplied?.(result);
      await loadProject();
    } catch (cause) {
      setProblem(errorText(cause));
    } finally {
      setBusy(false);
    }
  }, [projectPath, chosen, skin, onApplied, loadProject]);

  const scanVault = useCallback(async () => {
    setVault({ state: "loading" });
    try {
      setVault({ state: "ready", data: await api.fabVaultScan("") });
    } catch (cause) {
      setVault({ state: "error", message: errorText(cause) });
    }
  }, []);

  const runImport = useCallback(async () => {
    if (!projectPath || !importPack || !chosen) return;
    setBusy(true);
    setProblem(null);
    try {
      const result: IconImport = await api.fabImportIcons(
        projectPath,
        vault.state === "ready" ? vault.data.root : "",
        importPack.id,
        chosen.id,
        licence,
      );
      setNotice(
        result.imported.length === 0
          ? `${importPack.title} had nothing matching this HUD's icons.`
          : `Imported ${result.imported.length} icon${
              result.imported.length === 1 ? "" : "s"
            } from ${importPack.title}${
              result.unmatched.length > 0 ? `; ${result.unmatched.join(", ")} still unmatched.` : "."
            } Rebuild the HUD to use them.`,
      );
      setImportPack(null);
      await loadProject();
    } catch (cause) {
      setProblem(errorText(cause));
    } finally {
      setBusy(false);
    }
  }, [projectPath, importPack, chosen, vault, licence, loadProject]);

  if (library.state === "loading" || library.state === "idle") {
    return <div className="studio-dock-empty">Reading the HUD library…</div>;
  }
  if (library.state === "error") {
    return <div className="studio-dock-error">{library.message}</div>;
  }
  if (!projectPath) {
    return <div className="studio-dock-empty">Open a game to give it a HUD.</div>;
  }

  const view = library.data;
  const installed = project.state === "ready" ? project.data : null;

  return (
    <div className="hud-panel">
      <div className="hud-gallery" role="radiogroup" aria-label="HUD presets">
        {view.presets.map((entry) => {
          const active = chosen?.id === entry.id;
          const current = installed?.preset === entry.id;
          const previewSkin =
            view.skins.find((row) => row.id === (active ? skin || entry.skin : entry.skin)) ??
            view.skins[0];
          return (
            <button
              key={entry.id}
              type="button"
              role="radio"
              aria-checked={active}
              className={`hud-card${active ? " active" : ""}`}
              onClick={() => {
                setPreset(entry.id);
                setSkin("");
              }}
            >
              <HudPreview preset={entry} skin={previewSkin} />
              <div className="hud-card-head">
                <strong>{entry.title}</strong>
                {current ? <span className="hud-card-badge">installed</span> : null}
              </div>
              <p className="hud-card-purpose">{entry.purpose}</p>
              <div className="hud-card-budget" title="Elements the player sees every second">
                {Array.from({ length: view.max_persistent }, (_, index) => (
                  <i key={index} className={index < entry.persistent ? "on" : ""} />
                ))}
                <span>
                  {entry.persistent} of {view.max_persistent} always on
                </span>
              </div>
            </button>
          );
        })}
      </div>

      <aside className="hud-side">
        <section>
          <h4>Look</h4>
          <div className="hud-skins" role="radiogroup" aria-label="HUD skins">
            {view.skins.map((entry) => {
              const active = (skin || chosen?.skin) === entry.id;
              return (
                <button
                  key={entry.id}
                  type="button"
                  role="radio"
                  aria-checked={active}
                  className={`hud-skin${active ? " active" : ""}`}
                  onClick={() => setSkin(entry.id)}
                  title={entry.blurb}
                >
                  <span className="hud-swatch" style={{ background: entry.plate }}>
                    <i style={{ background: entry.accent }} />
                    <i style={{ background: entry.warn }} />
                    <i style={{ background: entry.text }} />
                  </span>
                  {entry.title}
                </button>
              );
            })}
          </div>
          {chosenSkin ? <p className="hud-skin-blurb">{chosenSkin.blurb}</p> : null}
        </section>

        <section>
          <h4>Art</h4>
          {missingIcons.length === 0 ? (
            <p className="hud-note">This game has art for every icon the HUD asks for.</p>
          ) : (
            <>
              <p className="hud-note">
                No art yet for {missingIcons.join(", ")}. The HUD reads fine without it — those
                readouts show their caption instead.
              </p>
              <button
                type="button"
                className="hud-secondary"
                onClick={() => void scanVault()}
                disabled={busy}
              >
                Find icons in my asset library
              </button>
            </>
          )}

          {vault.state === "loading" ? <p className="hud-note">Reading the library…</p> : null}
          {vault.state === "error" ? <p className="studio-dock-error">{vault.message}</p> : null}
          {vault.state === "ready" ? (
            <div className="hud-packs">
              <p className="hud-note">{vault.data.note}</p>
              {vault.data.packs
                .filter((pack) => pack.supplies_icons)
                .map((pack) => (
                  <button
                    key={pack.id}
                    type="button"
                    className="hud-pack"
                    onClick={() => setImportPack(pack)}
                    disabled={busy}
                  >
                    <strong>{pack.title}</strong>
                    <span>{pack.seller || pack.note}</span>
                  </button>
                ))}
              {/* Packs Godot cannot open are shown with the reason rather than hidden: a
                  pack that silently vanishes reads as a bug in Bhippi. */}
              {vault.data.packs.some((pack) => pack.usability === "unusable") ? (
                <details className="hud-unusable">
                  <summary>
                    {vault.data.packs.filter((pack) => pack.usability === "unusable").length} packs
                    Godot cannot open
                  </summary>
                  <ul>
                    {vault.data.packs
                      .filter((pack) => pack.usability === "unusable")
                      .map((pack) => (
                        <li key={pack.id}>
                          <strong>{pack.title}</strong> — {pack.note}
                        </li>
                      ))}
                  </ul>
                </details>
              ) : null}
            </div>
          ) : null}
        </section>

        <section className="hud-apply">
          {problem ? <p className="studio-dock-error">{problem}</p> : null}
          {notice ? <p className="hud-notice">{notice}</p> : null}
          <button type="button" className="hud-primary" onClick={() => void apply()} disabled={busy}>
            {busy
              ? "Working…"
              : installed?.installed
                ? `Rebuild as ${chosen?.title ?? "this HUD"}`
                : `Build the ${chosen?.title ?? ""} HUD`}
          </button>
          <p className="hud-note">
            Writes <code>scenes/hud.tscn</code> and its script, and instances it into the main
            scene. Undoable like any other edit.
          </p>
        </section>
      </aside>

      {importPack ? (
        <div className="hud-modal" role="dialog" aria-label="Import icons">
          <div className="hud-modal-body">
            <h4>Import from {importPack.title}</h4>
            <p>
              Bhippi cannot read the licence out of an asset pack, and a game will not export
              with an asset that cannot name its terms. State the licence this pack came under —
              it is written beside every file and printed on the credits page.
            </p>
            <label>
              Licence
              <input
                value={licence}
                onChange={(event) => setLicence(event.target.value)}
                spellCheck={false}
              />
            </label>
            {importPack.seller ? <p className="hud-note">Seller: {importPack.seller}</p> : null}
            <div className="hud-modal-actions">
              <button type="button" className="hud-secondary" onClick={() => setImportPack(null)}>
                Cancel
              </button>
              <button
                type="button"
                className="hud-primary"
                onClick={() => void runImport()}
                disabled={busy || licence.trim().length === 0}
              >
                Import icons
              </button>
            </div>
          </div>
        </div>
      ) : null}
    </div>
  );
}
