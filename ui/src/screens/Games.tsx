// The Games screen (GAD-018, docs/16 §4.2): every project you own, as a game.
//
// The card joins the project list with `list_workspace_sessions` — the same two sources the
// workspace rail already uses — and then asks Rust for what only the project itself knows:
// its poster, whether the folder is a Godot game, and why Play may not run. Nothing on this
// screen composes a path, reads a manifest or decides what is blocked (INV-073); the
// Tauri asset protocol is off, so even the poster arrives as a `data:` URL Rust built.

import { useCallback, useEffect, useMemo, useState } from "react";
import type { GameCardInfo, ProjectSummary, WorkspaceSession } from "../lib/ipc";
import { api } from "../lib/api";
import {
  IconCamera,
  IconExternal,
  IconFolder,
  IconPlay,
  IconPlus,
  IconSearch,
} from "../components/icons";
import { clipName, clipPath, relativeTime } from "../lib/format";
import { buildGameCards, posterGradient, posterInitial, projectKey, samePath } from "../lib/gameCards";

const describe = (error: unknown): string => {
  const message = (error as { message?: unknown } | null)?.message;
  return typeof message === "string" && message.length > 0 ? message : String(error);
};

export function Games({
  projects,
  sessions,
  sessionsError,
  activeProject,
  onOpen,
  onCreateGame,
  onRetry,
}: {
  /** `null` while the first load is in flight. */
  projects: ProjectSummary[] | null;
  sessions: WorkspaceSession[] | null;
  sessionsError: string | null;
  activeProject: ProjectSummary | null;
  onOpen: (project: ProjectSummary) => void;
  onCreateGame: () => void;
  onRetry: () => void;
}) {
  const [query, setQuery] = useState("");
  /// One `game_card_info` reply per project, keyed the way every other path here is keyed.
  /// A project with no entry yet simply has no poster and no pill — never a wrong one.
  const [info, setInfo] = useState<Record<string, GameCardInfo>>({});
  /// The project a Snapshot is running for, so its button can say so and not be pressed twice.
  const [snapping, setSnapping] = useState<string | null>(null);
  const [actionError, setActionError] = useState<string | null>(null);

  const cards = useMemo(
    () => buildGameCards(projects ?? [], sessions ?? []),
    [projects, sessions],
  );

  /// Ask Rust about each project in turn rather than all at once: the reply includes an
  /// install check, and a dozen of those in parallel on a cold start is a dozen probes for
  /// the same engine.
  const loadInfo = useCallback(async (paths: string[], stale: () => boolean) => {
    for (const path of paths) {
      if (stale()) return;
      try {
        const reply = await api.gameCardInfo(path);
        if (stale()) return;
        setInfo((current) => ({ ...current, [projectKey(path)]: reply }));
      } catch {
        // A card without its detail still shows its name, folder and sessions. An error
        // banner for a poster nobody asked for would be noise.
      }
    }
  }, []);

  useEffect(() => {
    let dead = false;
    void loadInfo(cards.map((card) => card.path), () => dead);
    return () => {
      dead = true;
    };
  }, [cards, loadInfo]);

  const shown = useMemo(() => {
    const needle = query.trim().toLowerCase();
    if (needle.length === 0) return cards;
    return cards.filter(
      (card) =>
        card.name.toLowerCase().includes(needle) || card.path.toLowerCase().includes(needle),
    );
  }, [cards, query]);

  const projectAt = useCallback(
    (path: string) => (projects ?? []).find((row) => samePath(row.path, path)) ?? null,
    [projects],
  );

  /// Play opens the game first: the engine runs inside the Studio viewport (ADR-0045), so
  /// starting it from here without going there would put a window where nothing is looking.
  const play = useCallback(
    (project: ProjectSummary) => {
      setActionError(null);
      onOpen(project);
      void api.godotEmbedPlay(project.path).catch((error) => {
        setActionError(`Could not play ${project.name}: ${describe(error)}`);
      });
    },
    [onOpen],
  );

  /// Snapshot runs the real capture and then re-reads the card, so the tile shows the frame
  /// that was just taken rather than the one it was already holding.
  const snapshot = useCallback(
    async (project: ProjectSummary) => {
      setActionError(null);
      setSnapping(project.path);
      try {
        await api.godotCapturePoster(project.path);
        const reply = await api.gameCardInfo(project.path);
        setInfo((current) => ({ ...current, [projectKey(project.path)]: reply }));
      } catch (error) {
        setActionError(`Could not snapshot ${project.name}: ${describe(error)}`);
      } finally {
        setSnapping(null);
      }
    },
    [],
  );

  const loading = projects === null;
  const failed = sessionsError !== null;

  return (
    <div className="pane">
      <div className="pane-inner games-inner">
        <header className="games-head">
          <div>
            <h1 className="screen-title">Games</h1>
            <p className="screen-sub">
              Every game in this workspace, with its last activity and its sessions.
            </p>
          </div>

          <div className="games-tools">
            <div className="games-search">
              <IconSearch size={13} />
              <input
                type="search"
                value={query}
                onChange={(event) => setQuery(event.target.value)}
                placeholder="Search games..."
                aria-label="Search games"
                disabled={loading}
              />
            </div>
            <button className="btn-accent games-new" onClick={onCreateGame}>
              <IconPlus size={13} /> Describe your game
            </button>
          </div>
        </header>

        {failed ? (
          <div className="plugin-banner error" role="alert">
            <span>Sessions could not load: {sessionsError}</span>
            <button onClick={onRetry}>Retry</button>
          </div>
        ) : null}

        {actionError ? (
          <div className="plugin-banner error" role="alert">
            <span>{actionError}</span>
            <button onClick={() => setActionError(null)}>Dismiss</button>
          </div>
        ) : null}

        {loading ? (
          <div className="games-grid" aria-busy="true">
            {[0, 1, 2, 3, 4, 5].map((slot) => (
              <div key={slot} className="game-card skeleton" />
            ))}
          </div>
        ) : shown.length === 0 ? (
          <div className="plugin-none">
            <p>
              {query.trim().length > 0
                ? "No games match this search."
                : "No games yet. Describe one and Bhippi will build it."}
            </p>
            {query.trim().length > 0 ? (
              <button className="btn-primary" onClick={() => setQuery("")}>
                Clear search
              </button>
            ) : (
              <button className="btn-accent" onClick={onCreateGame}>
                <IconPlus size={13} /> Describe your game
              </button>
            )}
          </div>
        ) : (
          <div className="games-grid">
            {shown.map((card) => {
              const project = projectAt(card.path);
              const detail = info[projectKey(card.path)] ?? null;
              const poster = detail?.poster_data_url ?? null;
              const isGodot = detail?.is_godot_project ?? null;
              const blocked = detail?.blocked_reason ?? null;
              const busy = snapping !== null && samePath(snapping, card.path);
              const current = activeProject !== null && samePath(activeProject.path, card.path);
              return (
                <article
                  className={`game-card${current ? " current" : ""}`}
                  key={card.path}
                  aria-label={card.name}
                >
                  <div className="game-card-poster">
                    {poster ? (
                      <img src={poster} alt={`${card.name} in play`} loading="lazy" />
                    ) : (
                      <span
                        className="game-card-placeholder"
                        style={{ backgroundImage: posterGradient(card.name) }}
                      >
                        <b aria-hidden="true">{posterInitial(card.name)}</b>
                      </span>
                    )}
                  </div>

                  <div className="game-card-body">
                    <h2 className="game-card-name">{clipName(detail?.title ?? card.name, 28)}</h2>
                    <p className="game-card-line" title={card.path}>
                      <IconFolder size={11} /> {clipPath(card.path, 40)}
                    </p>
                    <p className="game-card-line">
                      {card.lastActivity
                        ? `Last opened ${relativeTime(card.lastActivity)}`
                        : "No sessions yet"}
                      {" · "}
                      {card.sessionCount === 1 ? "1 session" : `${card.sessionCount} sessions`}
                    </p>
                    {isGodot === null ? null : (
                      <span className={`game-card-pill${isGodot ? " ok" : " warn"}`}>
                        {isGodot ? "Godot project" : "Not a Godot project yet"}
                      </span>
                    )}
                  </div>

                  <div className="game-card-actions">
                    <button
                      className="game-card-action primary"
                      onClick={() => project && onOpen(project)}
                      disabled={!project}
                    >
                      <IconExternal size={12} /> Open
                    </button>
                    <button
                      className="game-card-action"
                      onClick={() => project && play(project)}
                      disabled={!project || blocked !== null}
                      title={blocked ?? "Run the game in the Studio viewport"}
                    >
                      <IconPlay size={12} /> Play
                    </button>
                    <button
                      className="game-card-action"
                      onClick={() => project && void snapshot(project)}
                      disabled={!project || blocked !== null || busy}
                      title={blocked ?? "Photograph the running game and keep the frame"}
                    >
                      <IconCamera size={12} /> {busy ? "Snapping…" : "Snapshot"}
                    </button>
                  </div>
                </article>
              );
            })}
          </div>
        )}
      </div>
    </div>
  );
}
