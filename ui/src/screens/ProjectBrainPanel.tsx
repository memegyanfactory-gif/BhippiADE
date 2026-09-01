import { useCallback, useEffect, useState } from "react";
import type {
  AssetView,
  BrainStatus,
  IndexReport,
  ModuleCardView,
  PhysicsBodyView,
  SceneEntityView,
  SceneView,
  SymbolHit,
} from "../lib/ipc";
import { api } from "../lib/api";
import {
  IconBrain,
  IconClose,
  IconEngine,
  IconRefresh,
  IconSearch,
} from "../components/icons";

function describe(thrown: unknown): string {
  const value = thrown as { message?: string; hint?: string };
  return [value.message, value.hint].filter(Boolean).join(" — ") || "Something went wrong.";
}

/**
 * Project Brain panel (plan SEC. 9.3).
 *
 * Shows the structural/embedding index status for the active project, exposes the
 * manual "Rebuild Project Brain" repair action, and lets the user browse module
 * knowledge cards and rank-search symbols. Everything here reads from Rust.
 */
export function ProjectBrainPanel({ onClose }: { onClose: () => void }) {
  const [status, setStatus] = useState<BrainStatus | null>(null);
  const [report, setReport] = useState<IndexReport | null>(null);
  const [cards, setCards] = useState<ModuleCardView[] | null>(null);
  const [rebuilding, setRebuilding] = useState(false);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const [query, setQuery] = useState("");
  const [hits, setHits] = useState<SymbolHit[] | null>(null);
  const [searching, setSearching] = useState(false);
  const [searchError, setSearchError] = useState<string | null>(null);

  const [scenes, setScenes] = useState<SceneView[] | null>(null);
  const [openScene, setOpenScene] = useState<string | null>(null);
  const [entities, setEntities] = useState<SceneEntityView[] | null>(null);
  const [worldError, setWorldError] = useState<string | null>(null);

  const [assets, setAssets] = useState<AssetView[] | null>(null);
  const [openAsset, setOpenAsset] = useState<string | null>(null);
  const [assetUsage, setAssetUsage] = useState<string[] | null>(null);
  const [assetsError, setAssetsError] = useState<string | null>(null);
  const [indexingAssets, setIndexingAssets] = useState(false);

  const [physics, setPhysics] = useState<PhysicsBodyView[] | null>(null);
  const [openBody, setOpenBody] = useState<string | null>(null);
  const [physicsError, setPhysicsError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const [nextStatus, nextCards] = await Promise.all([
        api.brainStatus(),
        api.brainModuleCards(),
      ]);
      setStatus(nextStatus);
      setCards(nextCards);
    } catch (thrown) {
      setError(describe(thrown));
    } finally {
      setLoading(false);
    }
    try {
      setScenes(await api.worldScenes());
    } catch (thrown) {
      setWorldError(describe(thrown));
    }
    try {
      setAssets(await api.worldAssets());
    } catch (thrown) {
      setAssetsError(describe(thrown));
    }
    try {
      setPhysics(await api.worldPhysics());
    } catch (thrown) {
      setPhysicsError(describe(thrown));
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  useEffect(() => {
    const escape = (event: KeyboardEvent) => event.key === "Escape" && onClose();
    window.addEventListener("keydown", escape);
    return () => window.removeEventListener("keydown", escape);
  }, [onClose]);

  const rebuild = async () => {
    setRebuilding(true);
    setError(null);
    setReport(null);
    try {
      const next = await api.rebuildBrain();
      setReport(next);
      await refresh();
    } catch (thrown) {
      setError(describe(thrown));
    } finally {
      setRebuilding(false);
    }
  };

  const runSearch = async () => {
    const trimmed = query.trim();
    if (!trimmed) return;
    setSearching(true);
    setSearchError(null);
    setHits(null);
    try {
      const results = await api.brainSearch(trimmed, 20);
      setHits(results);
    } catch (thrown) {
      setSearchError(describe(thrown));
    } finally {
      setSearching(false);
    }
  };

  const toggleScene = async (sceneId: string) => {
    if (openScene === sceneId) {
      setOpenScene(null);
      setEntities(null);
      return;
    }
    setOpenScene(sceneId);
    setEntities(null);
    setWorldError(null);
    try {
      setEntities(await api.worldSceneEntities(sceneId));
    } catch (thrown) {
      setWorldError(describe(thrown));
    }
  };

  const callFindEntity = async (sceneId: string, name: string) => {
    const trimmed = name.trim();
    if (!trimmed) return;
    setWorldError(null);
    try {
      setEntities(await api.worldFindEntity(sceneId, trimmed));
    } catch (thrown) {
      setWorldError(describe(thrown));
    }
  };

  const indexAssets = async () => {
    setIndexingAssets(true);
    setAssetsError(null);
    try {
      await api.worldIndexAssets(status?.revision ?? 0);
      setAssets(await api.worldAssets());
    } catch (thrown) {
      setAssetsError(describe(thrown));
    } finally {
      setIndexingAssets(false);
    }
  };

  const toggleAsset = async (assetId: string) => {
    if (openAsset === assetId) {
      setOpenAsset(null);
      setAssetUsage(null);
      return;
    }
    setOpenAsset(assetId);
    setAssetUsage(null);
    setAssetsError(null);
    try {
      setAssetUsage(await api.worldAssetUsage(assetId));
    } catch (thrown) {
      setAssetsError(describe(thrown));
    }
  };

  const toggleBody = (bodyId: string) => {
    setOpenBody((current) => (current === bodyId ? null : bodyId));
  };

  const activeScene = scenes?.find((scene) => scene.scene_id === openScene) ?? null;

  return (
    <div
      className="dialog-backdrop"
      onMouseDown={(event) => event.target === event.currentTarget && onClose()}
    >
      <div className="brain-dialog" role="dialog" aria-label="Project Brain" aria-modal="true">
        <header>
          <span className="brain-mark">
            <IconBrain size={15} />
          </span>
          <span>
            <strong>Project Brain</strong>
            <small>{status?.index_version ?? "structural index + embeddings"}</small>
          </span>
          <span className="grow" />
          <button onClick={onClose} aria-label="Close">
            <IconClose size={13} />
          </button>
        </header>

        <p className="brain-blurb">
          Bhippi's persistent knowledge graph of this project: the structural index, embeddings,
          and per-module cards that ground chat, search, and research here.
        </p>

        {error ? (
          <div className="project-error" role="alert">
            {error}
          </div>
        ) : null}

        {loading && !status ? (
          <div className="brain-muted">Loading project brain…</div>
        ) : (
          <div className="brain-stats">
            <div className="brain-stat">
              <strong>{status?.symbol_count ?? 0}</strong>
              <small>symbols indexed</small>
            </div>
            <div className="brain-stat">
              <strong>{status?.module_names.length ?? 0}</strong>
              <small>modules</small>
            </div>
            <div className="brain-stat">
              <strong>{status?.revision ?? 0}</strong>
              <small>revision</small>
            </div>
            <div className="brain-stat">
              <strong className={status?.indexed ? "ok" : "idle"}>
                {status?.indexed ? "indexed" : "empty"}
              </strong>
              <small>index state</small>
            </div>
            <div className="brain-stat embed">
              <strong className="mono">{status?.embedding_model ?? "—"}</strong>
              <small>embedding model</small>
            </div>
          </div>
        )}

        {report ? (
          <div className="brain-report" role="status">
            Rebuilt: {report.files_scanned} files scanned, {report.files_changed} re-indexed,{" "}
            {report.files_removed} removed, {report.symbols_counted} symbols, revision{" "}
            {report.revision}.
          </div>
        ) : null}

        <div className="brain-toolbar">
          <button
            className="project-primary"
            onClick={() => void rebuild()}
            disabled={rebuilding}
          >
            <IconRefresh size={13} />
            {rebuilding ? "Rebuilding…" : "Rebuild Project Brain"}
          </button>
        </div>

        <section className="brain-section">
          <h3>
            <IconSearch size={13} /> Search symbols
          </h3>
          <div className="brain-search">
            <input
              value={query}
              placeholder="e.g. player movement, compute_damage"
              spellCheck={false}
              onChange={(event) => setQuery(event.target.value)}
              onKeyDown={(event) => event.key === "Enter" && void runSearch()}
              aria-label="Search project symbols"
            />
            <button onClick={() => void runSearch()} disabled={searching || !query.trim()}>
              {searching ? "Searching…" : "Go"}
            </button>
          </div>
          {searchError ? (
            <div className="project-error" role="alert">
              {searchError}
            </div>
          ) : null}
          {hits ? (
            hits.length > 0 ? (
              <ul className="brain-hits">
                {hits.map((hit) => (
                  <li key={`${hit.qualified_name}:${hit.start_line ?? 0}`}>
                    <span className="brain-kind">{hit.kind}</span>
                    <code>{hit.qualified_name}</code>
                    {hit.start_line != null ? (
                      <span className="brain-line">line {hit.start_line}</span>
                    ) : null}
                  </li>
                ))}
              </ul>
            ) : (
              <div className="brain-muted">No symbols matched that query.</div>
            )
          ) : null}
        </section>

        <section className="brain-section">
          <h3>
            <span className="brain-module-title">Module cards</span>
            <span className="grow light" />
            <span className="brain-count">
              {cards && cards.length > 0 ? `${cards.length} modules` : ""}
            </span>
          </h3>
          {cards && cards.length > 0 ? (
            <ul className="brain-modules">
              {cards.map((card) => (
                <li key={card.module_name}>
                  <div className="brain-module-head">
                    <code>{card.module_name}</code>
                    <span className="brain-count">
                      {card.symbol_count} symbols · ~{card.token_estimate} tokens
                    </span>
                  </div>
                  {card.description ? <p>{card.description}</p> : null}
                  {card.entry_points.length > 0 ? (
                    <div className="brain-tags">
                      {card.entry_points.map((ep) => (
                        <span key={ep} className="brain-entry">
                          {ep}
                        </span>
                      ))}
                    </div>
                  ) : null}
                </li>
              ))}
            </ul>
          ) : status && !status.indexed ? (
            <div className="brain-muted">
              Nothing indexed yet — press "Rebuild Project Brain" to scan this project.
            </div>
          ) : null}
        </section>

        <section className="brain-section">
          <h3>
            <IconEngine size={13} /> World Brain
            <span className="grow light" />
            <span className="brain-count">
              {scenes && scenes.length > 0 ? `${scenes.length} scenes` : ""}
            </span>
          </h3>
          {worldError ? (
            <div className="project-error" role="alert">
              {worldError}
            </div>
          ) : null}
          {scenes && scenes.length > 0 ? (
            <ul className="brain-scenes">
              {scenes.map((scene) => (
                <li key={scene.scene_id}>
                  <button
                    className="brain-scene-toggle"
                    onClick={() => void toggleScene(scene.scene_id)}
                  >
                    <span className="brain-scene-name">
                      {scene.name}
                      <span className="brain-count">
                        {scene.entity_count} entities · {scene.kind}
                      </span>
                    </span>
                    <span className="brain-count mono">{scene.rel_path}</span>
                  </button>
                  {openScene === scene.scene_id ? (
                    <div className="brain-scene-entities">
                      {activeScene ? (
                        <div className="brain-scene-find">
                          <input
                            placeholder={`Find "${activeScene.name}" entity by name`}
                            spellCheck={false}
                            onKeyDown={(event) => {
                              if (event.key === "Enter") {
                                void callFindEntity(
                                  activeScene.scene_id,
                                  (event.target as HTMLInputElement).value,
                                );
                              }
                            }}
                            aria-label="Find world entity by name"
                          />
                        </div>
                      ) : null}
                      {entities === null ? (
                        <div className="brain-muted">Loading entities…</div>
                      ) : entities.length > 0 ? (
                        <ul className="brain-entity-list">
                          {entities.map((entity) => (
                            <li key={entity.entity_id}>
                              <code>{entity.stable_path}</code>
                              {entity.component_names.length > 0 ? (
                                <span className="brain-tags">
                                  {entity.component_names.map((c) => (
                                    <span key={c} className="brain-entry">
                                      {c}
                                    </span>
                                  ))}
                                </span>
                              ) : null}
                            </li>
                          ))}
                        </ul>
                      ) : (
                        <div className="brain-muted">
                          No entities matched in this scene.
                        </div>
                      )}
                    </div>
                  ) : null}
                </li>
              ))}
            </ul>
          ) : (
            <div className="brain-muted">
              No scenes indexed yet — scenes appear here as the engine saves them.
            </div>
          )}

          <div className="brain-assets-head">
            <span className="brain-count light">Assets</span>
            <span className="grow" />
            <button
              className="btn-ghost"
              disabled={indexingAssets}
              onClick={() => void indexAssets()}
            >
              {indexingAssets ? "Scanning…" : "Re-index assets"}
            </button>
          </div>
          {assetsError ? (
            <div className="project-error" role="alert">
              {assetsError}
            </div>
          ) : null}
          {assets && assets.length > 0 ? (
            <ul className="brain-assets">
              {assets.map((asset) => (
                <li key={asset.asset_id}>
                  <button
                    className="brain-asset-toggle"
                    onClick={() => void toggleAsset(asset.asset_id)}
                  >
                    <span className="brain-scene-name">
                      {asset.rel_path}
                      <span className="brain-count">
                        {asset.kind} · {asset.license}
                      </span>
                    </span>
                    <span className="brain-count mono">{asset.size_bytes} B</span>
                  </button>
                  {openAsset === asset.asset_id ? (
                    <div className="brain-asset-usage">
                      {assetUsage === null ? (
                        <div className="brain-muted">Resolving usage…</div>
                      ) : assetUsage.length > 0 ? (
                        <ul className="brain-entity-list">
                          {assetUsage.map((scene) => (
                            <li key={scene}>
                              <code>used by {scene}</code>
                            </li>
                          ))}
                        </ul>
                      ) : (
                        <div className="brain-muted">
                          Not referenced by any scene.
                        </div>
                      )}
                    </div>
                  ) : null}
                </li>
              ))}
            </ul>
          ) : (
            <div className="brain-muted">
              No assets indexed yet — press "Re-index assets" to scan this project's
              assets/ folder.
            </div>
          )}

          <div className="brain-physics-head">
            <span className="brain-count light">Physics</span>
          </div>
          {physicsError ? (
            <div className="project-error" role="alert">
              {physicsError}
            </div>
          ) : null}
          {physics && physics.length > 0 ? (
            <ul className="brain-physics">
              {physics.map((body) => (
                <li key={body.entity_id}>
                  <button
                    className="brain-physics-toggle"
                    onClick={() => void toggleBody(body.entity_id)}
                  >
                    <span className="brain-scene-name">
                      {body.body_kind ?? "collider"}
                      {body.has_character_controller ? " · controller" : ""}
                      <span className="brain-count">
                        {body.mass != null ? `${body.mass} kg` : "no mass"}
                      </span>
                    </span>
                    <span className="brain-count mono">
                      {body.entity_id.slice(0, 12)}
                    </span>
                  </button>
                  {openBody === body.entity_id ? (
                    <div className="brain-physics-detail">
                      <ul className="brain-entity-list">
                        <li>
                          <code>scene {body.scene_id}</code>
                        </li>
                        {body.collider_shape ? (
                          <li>
                            <code>shape {body.collider_shape}</code>
                          </li>
                        ) : null}
                        {body.sensor === 1 ? (
                          <li>
                            <code>sensor</code>
                          </li>
                        ) : null}
                        {body.lock_rotation === 1 ? (
                          <li>
                            <code>lock rotation</code>
                          </li>
                        ) : null}
                      </ul>
                    </div>
                  ) : null}
                </li>
              ))}
            </ul>
          ) : (
            <div className="brain-muted">
              No physics bodies indexed — entities carry RigidBody / Collider
              components as the engine saves them.
            </div>
          )}
        </section>

        <footer>
          <span className="brain-hint">Grounds chat, search, and research in your code.</span>
          <span className="grow" />
          <button className="btn-ghost" onClick={onClose}>
            Close
          </button>
        </footer>
      </div>
    </div>
  );
}
