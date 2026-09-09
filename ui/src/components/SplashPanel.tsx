// The Splash tab of the Studio dock: describe a splash, give it a logo, build it into the
// game, and take it out again.
//
// Nothing here decides anything. Rust reads the brief, picks the palette, refuses a splash
// nobody could read, writes the scene and the script, and points the game at it; this file
// draws that answer and sends strings back (INV-051). The preview is painted from the very
// spec the builder lowers, so what the card shows is what the game will boot into — a
// mock-up that could drift from the build would be worse than no preview at all.

import { useCallback, useEffect, useMemo, useState } from "react";
import { open, save } from "@tauri-apps/plugin-dialog";
import { api } from "../lib/api";
import type {
  SplashApplyResult,
  SplashFavourite,
  SplashLibraryView,
  SplashProjectState,
  SplashSpec,
} from "../lib/ipc";

interface SplashPanelProps {
  projectPath?: string;
  /** The game's name, used as the title a fresh splash starts from. */
  gameName?: string;
  /** Told when a build lands, so the dock can refresh its assets and version lists. */
  onApplied?: (result: SplashApplyResult) => void;
}

type Loadable<T> =
  | { state: "idle" }
  | { state: "loading" }
  | { state: "ready"; data: T }
  | { state: "error"; message: string };

const IDLE: Loadable<never> = { state: "idle" };

/** What the logo import pre-fills. The user confirms it; Bhippi never assumes it. */
const SUGGESTED_LICENCE = "Own artwork";

function errorText(cause: unknown): string {
  const value = cause as { message?: string; hint?: string } | undefined;
  if (value && typeof value.message === "string") {
    return value.hint ? `${value.message} ${value.hint}` : value.message;
  }
  return String(cause);
}

/** Seconds, as a person reads them. */
function seconds(ms: number): string {
  return `${(ms / 1000).toFixed(1).replace(/\.0$/, "")}s`;
}

/**
 * The splash, drawn at the shape it will play at.
 *
 * The logo is shown as a reserved frame rather than the file itself: the panel has no way to
 * read a `res://` path, and inventing a picture here would be a preview that lies.
 */
function SplashPreview({ spec }: { spec: SplashSpec }) {
  const palette = spec.palette;
  // A splash may be the logo alone. An empty label still takes a line of height, which
  // pushes the logo off centre, so nothing is drawn where there is nothing to draw.
  const hasText = Boolean(spec.title.trim() || spec.tagline.trim());
  return (
    <div
      className={`splash-preview backdrop-${spec.backdrop} motion-${spec.motion}`}
      style={{ background: palette.background }}
      role="img"
      aria-label={`Splash preview for ${spec.title}`}
    >
      {spec.backdrop === "vignette" ? <div className="splash-preview-vignette" /> : null}
      <div className="splash-preview-stack">
        {spec.backdrop === "band" ? (
          <span className="splash-preview-band" style={{ background: palette.accent }} />
        ) : null}
        {spec.logo_res_path ? (
          <span className="splash-preview-logo" style={{ borderColor: palette.accent }}>
            <span style={{ color: palette.ink }}>logo</span>
          </span>
        ) : null}
        {spec.title.trim() ? (
          <span
            className="splash-preview-title"
            style={{ color: palette.ink, fontSize: `${Math.round(spec.title_font_size / 2.4)}px` }}
          >
            {spec.title}
          </span>
        ) : null}
        {hasText ? (
          <span className="splash-preview-rule" style={{ background: palette.accent }} />
        ) : null}
        {spec.tagline.trim() ? (
          <span className="splash-preview-tagline" style={{ color: palette.ink }}>
            {spec.tagline}
          </span>
        ) : null}
      </div>
    </div>
  );
}

/**
 * What the two steps of a build are called, in the order they run.
 *
 * They are named rather than merged into one spinner because they fail differently and take
 * very different amounts of time: reading the brief is a lookup and returns at once, while
 * building writes the scene and the script and waits for Godot to compile the script before
 * either is allowed to stay on disk. A single "Working…" would make the slow half look like
 * a hang.
 */
type SplashStep = "generating" | "building";

const STEP_CAPTION: Record<SplashStep, string> = {
  generating: "Reading the brief",
  building: "Building it into the game",
};

/**
 * The splash being made, drawn at the shape the real card will take.
 *
 * A skeleton of the same layout rather than a spinner in an empty box: the panel keeps its
 * height, so nothing below it jumps when the real preview replaces this, and the shape
 * already tells you what is coming.
 */
function SplashGenerating({ step, hasLogo }: { step: SplashStep; hasLogo: boolean }) {
  return (
    <div className="splash-preview is-generating" role="status" aria-live="polite">
      <span className="splash-generating-sheen" aria-hidden="true" />
      <div className="splash-preview-stack">
        {hasLogo ? <span className="splash-skeleton logo" aria-hidden="true" /> : null}
        <span className="splash-skeleton title" aria-hidden="true" />
        <span className="splash-skeleton rule" aria-hidden="true" />
        <span className="splash-skeleton tagline" aria-hidden="true" />
      </div>
      <span className="splash-generating-caption">
        <span className="splash-spinner" aria-hidden="true" />
        {STEP_CAPTION[step]}
        <span className="splash-generating-dots" aria-hidden="true">
          <i />
          <i />
          <i />
        </span>
      </span>
    </div>
  );
}

export function SplashPanel({ projectPath, gameName, onApplied }: SplashPanelProps) {
  const [library, setLibrary] = useState<Loadable<SplashLibraryView>>(IDLE);
  const [project, setProject] = useState<Loadable<SplashProjectState>>(IDLE);
  const [favourites, setFavourites] = useState<SplashFavourite[]>([]);

  const [brief, setBrief] = useState("");
  const [title, setTitle] = useState("");
  const [tagline, setTagline] = useState("");
  const [logo, setLogo] = useState<string | null>(null);
  const [spec, setSpec] = useState<SplashSpec | null>(null);

  const [busy, setBusy] = useState(false);
  const [step, setStep] = useState<SplashStep | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [problem, setProblem] = useState<string | null>(null);

  const loadLibrary = useCallback(async () => {
    setLibrary({ state: "loading" });
    try {
      setLibrary({ state: "ready", data: await api.splashLibrary() });
    } catch (cause) {
      setLibrary({ state: "error", message: errorText(cause) });
    }
  }, []);

  const loadProject = useCallback(async () => {
    if (!projectPath) return;
    setProject({ state: "loading" });
    try {
      const data = await api.splashProjectState(projectPath);
      setProject({ state: "ready", data });
      // Reopen on the splash this game already has, rather than on an empty form that
      // invites the user to overwrite it without seeing it first.
      if (data.spec) {
        setSpec(data.spec);
        setBrief(data.spec.brief);
        setTitle(data.spec.title);
        setTagline(data.spec.tagline);
        setLogo(data.spec.logo_res_path);
      }
    } catch (cause) {
      setProject({ state: "error", message: errorText(cause) });
    }
  }, [projectPath]);

  const loadFavourites = useCallback(async () => {
    try {
      setFavourites(await api.splashFavourites());
    } catch {
      // A studio database that will not open is reported by every other panel already;
      // an empty favourites strip is the honest thing to draw here.
      setFavourites([]);
    }
  }, []);

  useEffect(() => {
    void loadLibrary();
    void loadFavourites();
  }, [loadLibrary, loadFavourites]);

  useEffect(() => {
    void loadProject();
  }, [loadProject]);

  const installed = project.state === "ready" ? project.data : null;
  const effectiveTitle = title.trim() || gameName?.trim() || "";

  /**
   * Generate the splash and put it straight into the game.
   *
   * Two steps behind one press, in that order and never merged: the spec is always shown,
   * even when the build is refused. A gate that stops the build — no main scene to hand over
   * to, lettering nobody could read — must not also swallow the card the user just asked to
   * see, or the panel looks like it did nothing at all.
   */
  const generate = useCallback(async () => {
    setProblem(null);
    setNotice(null);
    setBusy(true);
    setStep("generating");
    try {
      const next = await api.splashGenerate(brief, effectiveTitle, tagline, logo);
      setTitle(next.title);

      if (!projectPath) {
        setSpec(next);
        setNotice(`Read as ${next.mood}. Open a game to build it in.`);
        return;
      }
      setStep("building");
      try {
        const result = await api.splashApply(projectPath, next, "user");
        setSpec(next);
        setNotice(
          `Read as ${next.mood}. The game now opens on it for ${seconds(
            next.duration_ms,
          )}, then goes to ${result.next_scene_res}.`,
        );
        onApplied?.(result);
        await loadProject();
      } catch (cause) {
        // The build was refused, but the card was still made. Showing it is what lets the
        // user act on the message instead of wondering whether anything happened at all.
        setSpec(next);
        setProblem(errorText(cause));
      }
    } catch (cause) {
      setProblem(errorText(cause));
    } finally {
      setBusy(false);
      setStep(null);
    }
  }, [brief, effectiveTitle, tagline, logo, projectPath, onApplied, loadProject]);

  const chooseLogo = useCallback(async () => {
    if (!projectPath) return;
    try {
      const picked = await open({
        multiple: false,
        title: "Choose a logo for the splash screen",
        filters: [{ name: "Image", extensions: ["png", "jpg", "jpeg", "webp", "svg"] }],
      });
      if (typeof picked !== "string" || !picked) return;
      setBusy(true);
      setProblem(null);
      const imported = await api.splashImportLogo(projectPath, picked, SUGGESTED_LICENCE);
      setLogo(imported.res_path);
      setSpec((current) => (current ? { ...current, logo_res_path: imported.res_path } : current));
      setNotice(`Logo added as ${imported.rel_path}, recorded under "${imported.licence}".`);
    } catch (cause) {
      setProblem(errorText(cause));
    } finally {
      setBusy(false);
    }
  }, [projectPath]);

  const clearLogo = useCallback(() => {
    setLogo(null);
    setSpec((current) => (current ? { ...current, logo_res_path: null } : current));
  }, []);

  const patch = useCallback((change: Partial<SplashSpec>) => {
    setSpec((current) => (current ? { ...current, ...change } : current));
  }, []);

  const build = useCallback(async () => {
    if (!projectPath || !spec) return;
    setBusy(true);
    // The same work the second half of `generate` does, so it says the same thing while it
    // runs: writing the scene and waiting for Godot to compile the script is the slow part
    // either way, and it is the part that most looks like a hang without a signal.
    setStep("building");
    setProblem(null);
    setNotice(null);
    try {
      const result = await api.splashApply(projectPath, { ...spec, logo_res_path: logo }, "user");
      setNotice(
        `${result.replaced ? "Rebuilt" : "Built"} the splash. The game now opens on it for ${seconds(
          spec.duration_ms,
        )}, then goes to ${result.next_scene_res}.`,
      );
      onApplied?.(result);
      await loadProject();
    } catch (cause) {
      setProblem(errorText(cause));
    } finally {
      setBusy(false);
      setStep(null);
    }
  }, [projectPath, spec, logo, onApplied, loadProject]);

  const favourite = useCallback(async () => {
    if (!spec) return;
    setBusy(true);
    setProblem(null);
    try {
      const saved = await api.splashFavourite(spec.title, spec, projectPath ?? "");
      setNotice(`Saved "${saved.name}" to your splashes.`);
      await loadFavourites();
    } catch (cause) {
      setProblem(errorText(cause));
    } finally {
      setBusy(false);
    }
  }, [spec, projectPath, loadFavourites]);

  const forget = useCallback(
    async (id: string) => {
      try {
        await api.splashForgetFavourite(id);
        await loadFavourites();
      } catch (cause) {
        setProblem(errorText(cause));
      }
    },
    [loadFavourites],
  );

  const exportSplash = useCallback(async () => {
    if (!projectPath || !spec) return;
    try {
      const picked = await save({
        title: "Export the splash screen into a folder",
        defaultPath: spec.title || "splash",
      });
      if (typeof picked !== "string" || !picked) return;
      setBusy(true);
      setProblem(null);
      const result = await api.splashExport(projectPath, picked, spec);
      setNotice(`Exported ${result.files.length} file${
        result.files.length === 1 ? "" : "s"
      } to ${result.folder}.`);
    } catch (cause) {
      setProblem(errorText(cause));
    } finally {
      setBusy(false);
    }
  }, [projectPath, spec]);

  const durationBounds = useMemo(
    () =>
      library.state === "ready"
        ? { min: library.data.min_duration_ms, max: library.data.max_duration_ms }
        : { min: 3000, max: 5000 },
    [library],
  );

  if (library.state === "loading" || library.state === "idle") {
    return <div className="studio-dock-empty">Reading the splash library…</div>;
  }
  if (library.state === "error") {
    return <div className="studio-dock-error">{library.message}</div>;
  }
  if (!projectPath) {
    return <div className="studio-dock-empty">Open a game to give it a splash screen.</div>;
  }

  const view = library.data;

  return (
    <div className="splash-panel">
      <div className="splash-form">
        <label className="splash-field">
          <span className="splash-label">Describe the splash screen</span>
          <textarea
            className="splash-brief"
            value={brief}
            maxLength={view.max_brief_chars}
            placeholder={
              "A neon cyberpunk title card, hold for 4 seconds, accent #00ff9c. Say what it " +
              "should feel like, the colours, how long it holds, and how it should animate."
            }
            onChange={(event) => setBrief(event.target.value)}
            rows={5}
          />
          <span className="splash-hint">
            {brief.length}/{view.max_brief_chars}
          </span>
        </label>

        <div className="splash-row">
          <label className="splash-field">
            <span className="splash-label">Title</span>
            <input
              className="splash-input"
              value={title}
              maxLength={view.max_title_chars}
              placeholder={gameName || "The game's name"}
              onChange={(event) => setTitle(event.target.value)}
            />
          </label>
          <label className="splash-field">
            <span className="splash-label">Tagline</span>
            <input
              className="splash-input"
              value={tagline}
              maxLength={view.max_tagline_chars}
              placeholder="Optional"
              onChange={(event) => setTagline(event.target.value)}
            />
          </label>
        </div>

        <div className="splash-logo-row">
          <button type="button" className="splash-btn" onClick={() => void chooseLogo()} disabled={busy}>
            {logo ? "Replace logo" : "Upload logo"}
          </button>
          {logo ? (
            <>
              <span className="splash-logo-name" title={logo}>
                {logo.replace("res://", "")}
              </span>
              <button type="button" className="splash-btn ghost" onClick={clearLogo} disabled={busy}>
                Remove
              </button>
            </>
          ) : (
            <span className="splash-hint">PNG, JPG, WebP or SVG. A licence is recorded with it.</span>
          )}
        </div>

        <button
          type="button"
          className="splash-btn primary"
          onClick={() => void generate()}
          disabled={busy}
        >
          {step ? (
            <>
              <span className="splash-spinner" aria-hidden="true" />
              {STEP_CAPTION[step]}…
            </>
          ) : (
            "Generate and use in game"
          )}
        </button>
      </div>

      {problem ? (
        <div className="splash-problem" role="alert">
          {problem}
        </div>
      ) : null}
      {notice ? <div className="splash-notice">{notice}</div> : null}

      {step ? (
        <SplashGenerating step={step} hasLogo={Boolean(logo)} />
      ) : spec ? (
        <div className="splash-result">
          <SplashPreview spec={{ ...spec, logo_res_path: logo }} />

          <div className="splash-controls">
            <div className="splash-control">
              <span className="splash-label">Holds for {seconds(spec.duration_ms)}</span>
              <input
                type="range"
                min={durationBounds.min}
                max={durationBounds.max}
                step={100}
                value={spec.duration_ms}
                onChange={(event) => patch({ duration_ms: Number(event.target.value) })}
              />
            </div>

            <div className="splash-control">
              <span className="splash-label">Motion</span>
              <div className="splash-choices">
                {view.motions.map((choice) => (
                  <button
                    key={choice.id}
                    type="button"
                    className={`splash-chip${spec.motion === choice.id ? " active" : ""}`}
                    onClick={() => patch({ motion: choice.id as SplashSpec["motion"] })}
                  >
                    {choice.title}
                  </button>
                ))}
              </div>
            </div>

            <div className="splash-control">
              <span className="splash-label">Backdrop</span>
              <div className="splash-choices">
                {view.backdrops.map((choice) => (
                  <button
                    key={choice.id}
                    type="button"
                    className={`splash-chip${spec.backdrop === choice.id ? " active" : ""}`}
                    onClick={() => patch({ backdrop: choice.id as SplashSpec["backdrop"] })}
                  >
                    {choice.title}
                  </button>
                ))}
              </div>
            </div>

            <div className="splash-control">
              <span className="splash-label">Colours</span>
              <div className="splash-swatches">
                {(["background", "ink", "accent"] as const).map((slot) => (
                  <label key={slot} className="splash-swatch" title={slot}>
                    <input
                      type="color"
                      value={spec.palette[slot]}
                      onChange={(event) =>
                        patch({ palette: { ...spec.palette, [slot]: event.target.value } })
                      }
                    />
                    <span>{slot}</span>
                  </label>
                ))}
              </div>
            </div>
          </div>

          <div className="splash-actions">
            <button
              type="button"
              className="splash-btn primary"
              onClick={() => void build()}
              disabled={busy}
            >
              {installed?.installed ? "Apply these changes" : "Build into the game"}
            </button>
            <button type="button" className="splash-btn" onClick={() => void favourite()} disabled={busy}>
              ★ Favourite
            </button>
            <button
              type="button"
              className="splash-btn"
              onClick={() => void exportSplash()}
              disabled={busy}
            >
              Export…
            </button>
          </div>

          {installed?.installed ? (
            <p className="splash-status">
              {installed.is_main_scene
                ? `This game opens on its splash, then goes to ${installed.next_scene_res}.`
                : "The splash scene exists but the game does not open on it yet. Build it again to wire it up."}
            </p>
          ) : null}
        </div>
      ) : (
        <p className="splash-status">
          Describe the splash and press Generate. It is built into this game straight away and
          plays at every start, for between {seconds(view.min_duration_ms)} and{" "}
          {seconds(view.max_duration_ms)}, and the player can skip it. A logo on its own is
          enough — the text is optional.
        </p>
      )}

      {favourites.length > 0 ? (
        <div className="splash-favourites">
          <span className="splash-label">Your splashes</span>
          <div className="splash-favourite-list">
            {favourites.map((entry) => (
              <div key={entry.id} className="splash-favourite">
                <button
                  type="button"
                  className="splash-favourite-open"
                  onClick={() => {
                    setSpec(entry.spec);
                    setBrief(entry.spec.brief);
                    setTitle(entry.spec.title);
                    setTagline(entry.spec.tagline);
                    setLogo(entry.spec.logo_res_path);
                    setNotice(`Loaded "${entry.name}".`);
                  }}
                  title={entry.origin ? `Saved from ${entry.origin}` : "Saved splash"}
                >
                  <span
                    className="splash-favourite-swatch"
                    style={{
                      background: entry.spec.palette.background,
                      borderColor: entry.spec.palette.accent,
                    }}
                  />
                  <span className="splash-favourite-name">{entry.name}</span>
                </button>
                <button
                  type="button"
                  className="splash-favourite-forget"
                  onClick={() => void forget(entry.id)}
                  aria-label={`Forget ${entry.name}`}
                  title="Forget this splash"
                >
                  ×
                </button>
              </div>
            ))}
          </div>
        </div>
      ) : null}
    </div>
  );
}
