import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type {
  EngineSceneState,
  EngineSceneDiff,
  EngineAssetView,
  EngineStatus,
  EngineTemplateView,
  InputDocument,
  HudWidgetView,
  RenderManifest,
} from "../lib/ipc";

/// The HUD overlay Play renders: widgets already placed by the engine.
type PlayHud = {
  widgets: HudWidgetView[];
  reference: [number, number];
  document: string;
};
import { api, events } from "../lib/api";
import {
  decodeSceneDocument,
  EMPTY_SCENE,
  type SceneDoc,
  type SceneEntity,
  type SceneTransform,
  type WeatherId,
  type WeatherPreset,
} from "./EngineSceneDocument";
import { EngineViewport, type PlayControls } from "./EngineViewport";
import {
  runScriptedPlaytest,
  type RuntimeEvent,
  type RuntimeStats,
  type ScriptedPlaytestStep,
} from "./playRuntime.ts";
// The compiled-program shape is generated from `bhippi-engine::script`, so the pane cannot
// drift from what the compiler emits; `scriptVm.ts` keeps its own structural copy only so
// the Node test harness can import it without the Tauri bindings.
import type { EngineCapabilityRow, ScriptFault, ScriptProgram } from "../lib/ipc";
import { EngineHierarchy } from "./EngineHierarchy";
import { EngineInspector } from "./EngineInspector";
import { EngineContentDrawer, type EngineDrawerTab } from "./EngineContentDrawer";
import { GAME_DEBUG_READY_EVENT, type GameDebugReadyDetail } from "./gameDebugUiEvent";
import { EngineHudEditor } from "./EngineHudEditor";
import { EngineCommandPalette, type PaletteCommand } from "./EngineCommandPalette";
import { EngineOutputLog, type LogLine } from "./EngineOutputLog";
import { requestOpenWorkspaceFile } from "../workbench/openFileRequest";
import { evictMissingRecent, rankQuickOpen } from "./quickOpen";
import {
  IconAlert,
  IconBadgeCheck,
  IconBox,
  IconCamera,
  IconEngine,
  IconGrid,
  IconLayers,
  IconPause,
  IconPlay,
  IconPlus,
  IconRefresh,
  IconStop,
  IconSun,
} from "../components/icons";

interface Props {
  projectPath: string;
  refreshToken: number;
  active?: boolean;
}

/** Icons for the Add menu, keyed by the engine's own template names. */
const TEMPLATE_ICONS: Record<string, JSX.Element> = {
  cube: <IconBox size={12} />,
  sphere: <IconBox size={12} />,
  plane: <IconBox size={12} />,
  light: <IconSun size={12} />,
  camera: <IconCamera size={12} />,
  player: <IconLayers size={12} />,
  trigger: <IconLayers size={12} />,
  empty: <IconLayers size={12} />,
};

/**
 * The Engine pane.
 *
 * It owns **no scene state**. Every edit is dispatched as an `EngineAction` through
 * `engine_apply_action`, which applies it as a transaction in `bhippi-engine`, journals it,
 * and returns the new `EngineSceneState` that this component renders (INV-070, INV-073).
 * That is what lets an AI edit land while the user has unsaved work open without either
 * side losing anything, and what puts both actors on one undo stack.
 */
export function EngineView({ projectPath, refreshToken, active = true }: Props) {
  const [status, setStatus] = useState<EngineStatus | null>(null);
  const [loading, setLoading] = useState(true);
  const [sceneNotice, setSceneNotice] = useState<string | null>(null);

  const [scene, setScene] = useState<EngineSceneState | null>(null);
  // Null is a scene/schema reset; an array lets the viewport retain every untouched object.
  const [viewportTouched, setViewportTouched] = useState<readonly string[] | null>(null);
  /// Multi-select (ENG-141/145). The first id is the "active" one the Details panel edits;
  /// the rest come along for delete, duplicate and align.
  const [selection, setSelection] = useState<string[]>([]);
  const [isGame, setIsGame] = useState(false);
  const [weatherMenu, setWeatherMenu] = useState(false);
  /// ENG-190: the project's own `[agent]` capability switches, read from Bhippi.game.toml.
  const [capabilityMenu, setCapabilityMenu] = useState(false);
  const [capabilities, setCapabilities] = useState<EngineCapabilityRow[]>([]);
  const [presets, setPresets] = useState<WeatherPreset[]>([]);
  const [templates, setTemplates] = useState<EngineTemplateView[]>([]);
  const [playDoc, setPlayDoc] = useState<SceneDoc | null>(null);
  /// The HUD overlay Play draws, with rects already resolved by the engine.
  const [playHud, setPlayHud] = useState<PlayHud | null>(null);
  const [playConfig, setPlayConfig] = useState<{
    gravity: [number, number, number];
    input: InputDocument;
    levels: string[];
    /// Programs `bhippi-engine::script` compiled for this world, by entity id (ADR-0030).
    scripts: Map<string, ScriptProgram>;
  } | null>(null);
  /// Pause the sim on the frame a script faults. Off by default: a fault is loud in the
  /// Output Log either way, and stopping the world for a cosmetic script is worse.
  const [pauseOnScriptError, setPauseOnScriptError] = useState(false);

  // Unreal-style editor modes. These are view state, not scene state, so they stay local.
  const [isPlaying, setIsPlaying] = useState(false);
  const [playPaused, setPlayPaused] = useState(false);
  const [playStepToken, setPlayStepToken] = useState(0);
  const [playRestartToken, setPlayRestartToken] = useState(0);
  const [playTimeScale, setPlayTimeScale] = useState(1);
  const [gameView, setGameView] = useState(false);
  const [playEjected, setPlayEjected] = useState(false);
  const [playStats, setPlayStats] = useState<(RuntimeStats & { drawCalls: number }) | null>(null);
  const [gizmoMode, setGizmoMode] = useState<"select" | "translate" | "rotate" | "scale">("translate");
  const [gizmoSpace, setGizmoSpace] = useState<"world" | "local">("world");
  const [snap, setSnap] = useState<number | null>(1);
  const [shadingMode, setShadingMode] = useState<"lit" | "unlit" | "wireframe" | "detail_lighting" | "lighting_only" | "collision">("lit");
  const wireframe = shadingMode === "wireframe";
  const [cameraMode, setCameraMode] = useState<"perspective" | "top" | "bottom" | "front" | "back" | "left" | "right">("perspective");
  const [viewportFov, setViewportFov] = useState(58);
  const [screenPercentage, setScreenPercentage] = useState(100);
  const [viewportMaximized, setViewportMaximized] = useState(false);
  const [isDrawerCollapsed, setIsDrawerCollapsed] = useState(() => readDrawerPreference(projectPath).collapsed);
  const [drawerTab, setDrawerTab] = useState<EngineDrawerTab>(() => readDrawerPreference(projectPath).tab);
  const [drawerHeight, setDrawerHeight] = useState(() => readDrawerPreference(projectPath).height);
  const [narrowInspectorOpen, setNarrowInspectorOpen] = useState(false);
  const [narrowFocus, setNarrowFocus] = useState<"world" | "viewport" | "details">("viewport");
  const [gameDebugRefreshToken, setGameDebugRefreshToken] = useState(0);
  const [showAddMenu, setShowAddMenu] = useState(false);
  const [savedFeedback, setSavedFeedback] = useState(false);
  // How much the agent may change without asking (ENG-116). It lives on the engine toolbar
  // rather than in Settings because this is where the user is when the question matters.
  const [agentMode, setAgentMode] = useState<string>("auto");
  /// Scene editing or HUD editing. The HUD is its own document (`bhippi-hud@1`), so it gets
  /// its own editor rather than pretending to be a 3D scene.
  const [editorTab, setEditorTab] = useState<"scene" | "hud">("scene");
  const [paletteOpen, setPaletteOpen] = useState(false);
  const [paletteMode, setPaletteMode] = useState<"commands" | "assets">("commands");
  const [paletteAssets, setPaletteAssets] = useState<EngineAssetView[]>([]);
  const [paletteAssetsLoaded, setPaletteAssetsLoaded] = useState(false);
  const recentStorageKey = `bhippi.engine.quick-open.${projectPath}`;
  const [recentQuickOpen, setRecentQuickOpen] = useState<string[]>(() => {
    try {
      return JSON.parse(localStorage.getItem(recentStorageKey) ?? "[]") as string[];
    } catch {
      return [];
    }
  });
  /// Viewport Show flags (ENG-144) — what the editor draws over the scene.
  const [showFlags, setShowFlags] = useState({ grid: true, icons: true, bounds: false, colliders: false });
  const [showMenu, setShowMenu] = useState(false);
  const [playOptionsOpen, setPlayOptionsOpen] = useState(false);
  const [toolbarMoreOpen, setToolbarMoreOpen] = useState(false);
  const [viewportOptionsOpen, setViewportOptionsOpen] = useState(false);
  /// Fly-camera speed, the way UE5 exposes it on the viewport toolbar.
  const [cameraSpeed, setCameraSpeed] = useState(1);
  /// Notices the pane raised, kept for the Output Log (ENG-149). The journal half of the
  /// log comes from the database, so this only holds what the editor itself said.
  const [logLines, setLogLines] = useState<LogLine[]>([]);
  /// Monotonic, because two log lines in the same millisecond must not share a React key.
  const logSequence = useRef(0);
  /// `handleRuntimeEvent` is declared before `loadPlayLevel` and must not capture a stale
  /// one; a ref is the smaller of the two evils against reordering the whole block.
  const loadPlayLevelRef = useRef<((level: string) => Promise<void>) | null>(null);
  const logOpen = !isDrawerCollapsed && drawerTab === "output";
  const [sceneDiff, setSceneDiff] = useState<EngineSceneDiff | null>(null);
  /// The most recent applied change, shown as a toast with an Undo affordance (ENG-150).
  const [toast, setToast] = useState<{ label: string; actor: string } | null>(null);
  /// Meshes and materials resolved by the engine for the open scene (ENG-160/162).
  const [manifest, setManifest] = useState<RenderManifest | null>(null);

  const activeScenePath = scene?.scene_path ?? "";
  const doc = useMemo(
    () => (scene ? decodeSceneDocument(scene.document_json) : EMPTY_SCENE),
    [scene],
  );
  const scenePathRef = useRef(activeScenePath);
  useEffect(() => {
    scenePathRef.current = activeScenePath;
  }, [activeScenePath]);

  const selectedId = selection[0] ?? null;

  useEffect(() => {
    localStorage.setItem(
      `bhippi.engine.drawer.${projectPath}`,
      JSON.stringify({ collapsed: isDrawerCollapsed, tab: drawerTab, height: drawerHeight }),
    );
  }, [drawerHeight, drawerTab, isDrawerCollapsed, projectPath]);

  useEffect(() => {
    const onReady = (raw: Event) => {
      const event = raw as CustomEvent<GameDebugReadyDetail>;
      if (event.detail.projectPath !== projectPath) return;
      setGameDebugRefreshToken((token) => token + 1);
      setDrawerTab("game-debug");
      setIsDrawerCollapsed(false);
    };
    window.addEventListener(GAME_DEBUG_READY_EVENT, onReady);
    return () => window.removeEventListener(GAME_DEBUG_READY_EVENT, onReady);
  }, [projectPath]);

  const toggleDrawerTab = useCallback((tab: EngineDrawerTab) => {
    if (drawerTab === tab && !isDrawerCollapsed) {
      setIsDrawerCollapsed(true);
      return;
    }
    setDrawerTab(tab);
    setIsDrawerCollapsed(false);
  }, [drawerTab, isDrawerCollapsed]);

  const log = useCallback((level: LogLine["level"], channel: string, text: string) => {
    setLogLines((current) =>
      [
        ...current,
        { id: `${Date.now()}-${current.length}`, at: new Date().toISOString(), level, channel, text },
      ].slice(-300),
    );
    void api.engineRecordConsole(level, channel, text).catch(() => undefined);
  }, []);

  const report = useCallback(
    (error: any, verb: string) => {
      const message = error?.message ?? String(error);
      const hint = error?.hint ? ` ${error.hint}` : "";
      const text = `Could not ${verb}: ${message}${hint}`;
      setSceneNotice(text);
      log("error", "editor", text);
    },
    [log],
  );

  const openScene = useCallback(
    async (path: string | null) => {
      try {
        const next = await api.engineOpenScene(path);
        setViewportTouched(null);
        setScene(next);
        setSelection((current) => {
          const decoded = decodeSceneDocument(next.document_json);
          const alive = current.filter((id) => decoded.entities.some((entity) => entity.id === id));
          if (alive.length > 0) return alive;
          const first = decoded.entities[0]?.id;
          return first ? [first] : [];
        });
        return next;
      } catch (error: any) {
        report(error, `open ${path ?? "the default scene"}`);
        return null;
      }
    },
    [report],
  );

  const reload = useCallback(async () => {
    setLoading(true);
    setSceneNotice(null);
    setIsPlaying(false);
    setPlayDoc(null);
    setPlayHud(null);
    try {
      const next = await api.engineStatus();
      setStatus(next);
      const game = next.game ?? null;
      setIsGame(!!game);
      if (!game) {
        setViewportTouched(null);
        setScene(null);
        setSelection([]);
        return;
      }
      await openScene(null);
    } catch {
      setIsGame(false);
      setViewportTouched(null);
      setScene(null);
      setSelection([]);
    } finally {
      setLoading(false);
    }
    // `openScene` closes over the previous status, which is exactly what we want on the
    // first pass: the manifest we just fetched is passed to it explicitly below.
  }, [openScene]);

  useEffect(() => {
    void reload();
  }, [projectPath, refreshToken, reload]);

  useEffect(() => {
    if (!isGame) {
      setPaletteAssets([]);
      setPaletteAssetsLoaded(true);
      return;
    }
    setPaletteAssetsLoaded(false);
    let cancelled = false;
    void api.engineListAssets()
      .then((assets) => {
        if (!cancelled) {
          setPaletteAssets(assets);
          setPaletteAssetsLoaded(true);
        }
      })
      .catch(() => {
        if (!cancelled) {
          setPaletteAssets([]);
          setPaletteAssetsLoaded(true);
        }
      });
    return () => { cancelled = true; };
  }, [isGame, scene?.revision]);

  useEffect(() => {
    setPaletteAssetsLoaded(false);
    try {
      const parsed = JSON.parse(localStorage.getItem(recentStorageKey) ?? "[]");
      setRecentQuickOpen(Array.isArray(parsed) ? parsed.filter((value) => typeof value === "string") : []);
    } catch {
      setRecentQuickOpen([]);
    }
  }, [recentStorageKey]);

  const rememberQuickOpen = useCallback((path: string) => {
    setRecentQuickOpen((current) => {
      const next = [path, ...current.filter((entry) => entry !== path)].slice(0, 20);
      localStorage.setItem(recentStorageKey, JSON.stringify(next));
      return next;
    });
  }, [recentStorageKey]);

  useEffect(() => {
    if (!isGame || !activeScenePath) {
      setManifest(null);
      return;
    }
    let cancelled = false;
    void api
      .engineRenderManifest(activeScenePath)
      .then((next) => {
        if (cancelled) return;
        setManifest(next);
        if (next.missing.length > 0) {
          log(
            "warn",
            "assets",
            `${next.missing.length} reference(s) do not resolve: ${next.missing.join(", ")}`,
          );
        }
      })
      .catch(() => {
        if (!cancelled) setManifest(null);
      });
    return () => {
      cancelled = true;
    };
    // `scene?.revision` is the trigger: any applied transaction may have added a material.
  }, [activeScenePath, isGame, log, scene?.revision]);

  // A toast is a notification, not a dialog: it goes away on its own.
  useEffect(() => {
    if (!toast) return;
    const timer = window.setTimeout(() => setToast(null), 5000);
    return () => window.clearTimeout(timer);
  }, [toast]);

  // The engine's registries — the palette and the weather presets are data owned by
  // `bhippi-engine`, fetched once instead of duplicated in this file (ENG-105).
  useEffect(() => {
    void (async () => {
      try {
        const [weather, palette] = await Promise.all([
          api.engineWeatherPresets(),
          api.engineTemplates(),
        ]);
        setPresets(weather as WeatherPreset[]);
        setTemplates(palette);
      } catch {
        setPresets([]);
        setTemplates([]);
      }
      try {
        setAgentMode(await api.enginePermissionMode());
      } catch {
        setAgentMode("auto");
      }
    })();
  }, []);

  // An edit from anywhere re-reads the live session (never disk), but rapid events are
  // coalesced into one frame. Revisions are monotonic, so a slower response can never
  // overwrite the final transform from a newer transaction (ENG-107).
  useEffect(() => {
    let timer: number | null = null;
    let inFlight = false;
    let disposed = false;
    let pendingRevision = -1;
    let pendingFull = false;
    const pendingTouched = new Set<string>();

    const schedule = () => {
      if (timer !== null || inFlight || disposed) return;
      timer = window.setTimeout(() => {
        timer = null;
        void flush();
      }, 16);
    };
    const flush = async () => {
      if (inFlight || disposed || pendingRevision < 0) return;
      inFlight = true;
      const revision = pendingRevision;
      const touched = pendingFull ? null : [...pendingTouched];
      pendingRevision = -1;
      pendingFull = false;
      pendingTouched.clear();
      try {
        const next = await api.engineOpenScene(scenePathRef.current || null);
        if (!disposed && next.revision >= revision) {
          setViewportTouched(touched);
          setScene((current) => !current || next.revision >= current.revision ? next : current);
        }
      } catch {
        /* the scene was closed underneath us; the next explicit open reports it */
      } finally {
        inFlight = false;
        if (pendingRevision >= 0) schedule();
      }
    };
    const unlisten = events.engineSceneChanged.listen((event) => {
      const changed = event.payload.scene_path;
      if (changed && scenePathRef.current && changed !== scenePathRef.current) return;
      if (event.payload.actor === "agent") {
        setToast({ label: event.payload.label, actor: "agent" });
      }
      pendingRevision = Math.max(pendingRevision, event.payload.revision);
      if (event.payload.touched.length === 0) pendingFull = true;
      for (const id of event.payload.touched) pendingTouched.add(id);
      schedule();
    });
    return () => {
      disposed = true;
      if (timer !== null) window.clearTimeout(timer);
      void unlisten.then((off) => off());
    };
  }, []);

  /** Dispatch one engine action and adopt the state it returns. */
  const applyAction = useCallback(
    async (action: Record<string, unknown>, label: string) => {
      if (!isGame) {
        setSceneNotice("This folder is not a game. Use New Game to scaffold Main, HUD, and Level 1.");
        return null;
      }
      try {
        const result = await api.engineApplyAction(
          JSON.stringify(action),
          activeScenePath || null,
          label,
        );
        setViewportTouched(result.touched);
        setScene(result.state);
        setSceneNotice(null);
        setToast({ label: result.label, actor: result.actor });
        log("info", result.actor, `${result.label} · ${result.summary}`);
        return result;
      } catch (error: any) {
        report(error, label);
        return null;
      }
    },
    [activeScenePath, isGame, log, report],
  );

  const handleSelectScene = useCallback(
    async (relPath: string) => {
      setSceneNotice(null);
      setIsPlaying(false);
      setPlayDoc(null);
      setPlayHud(null);
      await openScene(relPath);
    },
    [openScene],
  );

  const handleSaveScene = useCallback(async () => {
    if (!isGame || !activeScenePath) {
      setSceneNotice("This folder is not a game. Use New Game to scaffold Main, HUD, and Level 1.");
      return;
    }
    try {
      const next = await api.engineSaveScene(activeScenePath);
      setScene(next);
      setSceneNotice(null);
      setSavedFeedback(true);
      window.setTimeout(() => setSavedFeedback(false), 2400);
    } catch (error: any) {
      report(error, `save ${activeScenePath}`);
    }
  }, [activeScenePath, isGame, report]);

  const handleNewGame = useCallback(async () => {
    try {
      const next = await api.createGameManifest(null, true);
      setStatus(next);
      setIsGame(!!next.game);
      setSceneNotice("Game scaffolded: Main, HUD, and Level 1.");
      await reload();
    } catch (error: any) {
      report(error, "create a game");
    }
  }, [reload, report]);

  const undoEdit = useCallback(async () => {
    if (!isGame || !scene?.can_undo) return;
    try {
      setScene(await api.engineUndo(activeScenePath || null));
    } catch (error: any) {
      report(error, "undo");
    }
  }, [activeScenePath, isGame, report, scene?.can_undo]);

  const redoEdit = useCallback(async () => {
    if (!isGame || !scene?.can_redo) return;
    try {
      setScene(await api.engineRedo(activeScenePath || null));
    } catch (error: any) {
      report(error, "redo");
    }
  }, [activeScenePath, isGame, report, scene?.can_redo]);

  const handleWeather = useCallback(
    async (id: WeatherId | string) => {
      setWeatherMenu(false);
      await applyAction({ kind: "set_weather", weather: id }, `set weather ${id}`);
    },
    [applyAction],
  );

  const handleTakeDisk = useCallback(async () => {
    if (!activeScenePath) return;
    try {
      setScene(await api.engineReloadScene(activeScenePath));
      setSceneNotice(null);
    } catch (error: any) {
      report(error, `reload ${activeScenePath}`);
    }
  }, [activeScenePath, report]);

  const handleShowDiff = useCallback(async () => {
    if (!activeScenePath) return;
    try {
      setSceneDiff(await api.engineSceneDiff(activeScenePath));
    } catch (error: any) {
      report(error, "compare the scene conflict");
    }
  }, [activeScenePath, report]);

  const startPlay = useCallback(async () => {
    try {
      const world = await api.enginePlayWorld(activeScenePath || null);
      setPlayDoc(decodeSceneDocument(world.document_json));
      setPlayHud(
        world.hud_json
          ? {
              widgets: world.hud_widgets,
              reference: world.hud_reference,
              document: world.hud_json,
            }
          : null,
      );
      setPlayConfig({
        gravity: world.gravity,
        input: world.input,
        levels: world.levels,
        scripts: new Map(world.scripts.map((entry) => [entry.entity, entry.program])),
      });
      reportScriptFaults(world.script_faults, world.scripts.length);
      setPlayPaused(false);
      setPlayEjected(false);
      setPlayStats(null);
      setIsPlaying(true);
    } catch (error: any) {
      report(error, "start play");
    }
  }, [activeScenePath, report]);

  const stopPlay = useCallback(() => {
    setIsPlaying(false);
    setPlayOptionsOpen(false);
    setPlayDoc(null);
    setPlayHud(null);
    setPlayConfig(null);
    setPlayPaused(false);
    setPlayEjected(false);
    setPlayStats(null);
    void api.engineClearPlayStats().catch(() => undefined);
  }, []);

  const recoverScene = useCallback(async () => {
    if (!activeScenePath) return;
    try {
      setScene(await api.engineRecoverScene(activeScenePath));
      setSceneNotice("Recovered the crash snapshot. Review it, then Save to keep it.");
    } catch (error: any) {
      report(error, "recover scene");
    }
  }, [activeScenePath, report]);

  const viewDoc = isPlaying && playDoc ? playDoc : doc;

  const togglePlayPause = useCallback(() => setPlayPaused((value) => !value), []);
  /// Push one line onto the pane's half of the Output Log.
  const pushLog = useCallback((
    level: LogLine["level"],
    channel: string,
    text: string,
    source?: { path: string; line: number },
  ) => {
    logSequence.current += 1;
    const line: LogLine = {
      id: `runtime-${logSequence.current}`,
      at: new Date().toISOString(),
      level,
      channel,
      text,
      source,
    };
    // Bounded, because a script logging every frame must not grow without limit; the tail is
    // what anyone reads anyway.
    setLogLines((current) => [...current, line].slice(-500));
    if (level === "error") {
      setDrawerTab("problems");
      setIsDrawerCollapsed(false);
    }
    if (source) {
      void api.engineRecordConsoleSource(level, channel, text, source.path, source.line).catch(() => undefined);
    } else {
      void api.engineRecordConsole(level, channel, text).catch(() => undefined);
    }
  }, []);

  /// Compile faults from `engine_play_world` (ADR-0030). Play still starts — those entities
  /// run unscripted — so this has to be visible rather than silent.
  const reportScriptFaults = useCallback(
    (faults: ScriptFault[], compiled: number) => {
      for (const fault of faults) {
        pushLog(
          "error",
          "script",
          `${fault.file}:${fault.line}:${fault.column} ${fault.message}${fault.hint ? ` — ${fault.hint}` : ""}`,
          { path: fault.file, line: fault.line },
        );
      }
      if (faults.length > 0) {
        setSceneNotice(
          `${faults.length} script${faults.length === 1 ? "" : "s"} did not compile; those entities run unscripted. See the Output Log.`,
        );
      } else if (compiled > 0) {
        pushLog("info", "script", `Compiled ${compiled} script${compiled === 1 ? "" : "s"}.`);
      }
    },
    [pushLog],
  );

  // ENG-187: execute a bounded input script against the same deterministic PlayRuntime the
  // viewport uses. It runs on a disposable world returned by Rust and reports sampled state;
  // nothing is committed to the authored document.
  useEffect(() => {
    const unlisten = events.enginePlaytestRequested.listen((event) => {
      if (!active || !isGame) return;
      void (async () => {
        try {
          const world = await api.enginePlayWorld(activeScenePath || null);
          const steps = JSON.parse(event.payload.steps_json) as ScriptedPlaytestStep[];
          const report = runScriptedPlaytest(
            decodeSceneDocument(world.document_json),
            world.gravity,
            world.input,
            new Map(world.scripts.map((entry) => [entry.entity, entry.program])),
            steps,
            event.payload.fixed_delta_seconds,
          );
          await api.engineSubmitPlaytest(event.payload.request_id, JSON.stringify(report, null, 2));
          pushLog(
            report.faults.length > 0 ? "error" : "info",
            "playtest",
            `Agent playtest ran ${report.frames} frames across ${report.samples.length} input steps${report.faults.length > 0 ? ` and found ${report.faults.length} fault(s)` : ""}.`,
          );
        } catch (error) {
          await api
            .engineSubmitPlaytest(
              event.payload.request_id,
              JSON.stringify({ error: String(error), authoredUnchanged: true }),
            )
            .catch(() => undefined);
        }
      })();
    });
    return () => {
      void unlisten.then((off) => off());
    };
  }, [active, activeScenePath, isGame, pushLog]);

  const handleRuntimeEvent = useCallback(
    (event: RuntimeEvent) => {
      switch (event.kind) {
        case "fault":
          setSceneNotice(`${event.message} — ${event.hint}`);
          pushLog("error", "runtime", `${event.message} — ${event.hint}`);
          setPlayPaused(true);
          return;
        case "script_fault":
          pushLog(
            "error",
            "script",
            `${event.file}:${event.line} in ${event.hook} on ${event.entity}: ${event.message}${event.hint ? ` — ${event.hint}` : ""}`,
            { path: event.file, line: event.line },
          );
          setSceneNotice(`${event.file}:${event.line} — ${event.message} The script is disabled for this session.`);
          return;
        case "log":
          pushLog("info", "script", event.message);
          return;
        case "trigger":
          pushLog("info", "runtime", `Trigger ${event.other} entered by ${event.entity}`);
          return;
        case "level":
          void loadPlayLevelRef.current?.(event.name);
          return;
        case "sound":
          pushLog("info", "runtime", `Play sound ${event.asset}`);
          return;
        default:
          // Collisions, spawns and destroys are far too frequent to log; they are visible in
          // the viewport and counted in the stats.
          return;
      }
    },
    [pushLog],
  );
  const loadPlayLevel = useCallback(async (requested: string) => {
    const level = playConfig?.levels.find(
      (path) => path === requested || path.endsWith(`/${requested}`) || path.endsWith(`/${requested}.bscn.json`),
    );
    if (!level) {
      setSceneNotice(`Runtime refused unknown level “${requested}”. Add it to Bhippi.game.toml levels first.`);
      setPlayPaused(true);
      return;
    }
    try {
      const world = await api.enginePlayWorld(level);
      setPlayDoc(decodeSceneDocument(world.document_json));
      setPlayConfig({
        gravity: world.gravity,
        input: world.input,
        levels: world.levels,
        scripts: new Map(world.scripts.map((entry) => [entry.entity, entry.program])),
      });
      reportScriptFaults(world.script_faults, world.scripts.length);
      setPlayRestartToken((value) => value + 1);
      setSceneNotice(`Loaded ${level}; Main and HUD stayed persistent.`);
    } catch (error: any) {
      report(error, `load level ${requested}`);
      setPlayPaused(true);
    }
  }, [playConfig?.levels, report]);
  useEffect(() => {
    loadPlayLevelRef.current = loadPlayLevel;
  }, [loadPlayLevel]);

  const runtimeControls = useMemo<PlayControls | null>(
    () =>
      playConfig
        ? {
            paused: playPaused,
            stepToken: playStepToken,
            restartToken: playRestartToken,
            timeScale: playTimeScale,
            gameView,
            ejected: playEjected,
            gravity: playConfig.gravity,
            input: playConfig.input,
            scripts: playConfig.scripts,
            pauseOnError: pauseOnScriptError,
            onTogglePause: togglePlayPause,
            onStats: (stats) => {
              setPlayStats(stats);
              void api.engineRecordPlayStats({
                fps: stats.fps,
                frame_ms: stats.frameMs,
                entities: stats.entities,
                simulated_bodies: stats.simulatedBodies,
                contacts: stats.contacts,
                draw_calls: stats.drawCalls,
                scripts: stats.scripts,
                script_faults: stats.scriptFaults,
                elapsed: stats.elapsed,
                paused: stats.paused,
              }).catch(() => undefined);
            },
            onEvent: handleRuntimeEvent,
            onStop: stopPlay,
            onLoadLevel: (level) => void loadPlayLevel(level),
          }
        : null,
    [gameView, handleRuntimeEvent, loadPlayLevel, pauseOnScriptError, playConfig, playEjected, playPaused, playRestartToken, playStepToken, playTimeScale, stopPlay, togglePlayPause],
  );

  const handleAddEntity = useCallback(
    async (template: string) => {
      setShowAddMenu(false);
      const result = await applyAction({ kind: "spawn", template }, `spawn ${template}`);
      const created = result?.touched?.[0];
      if (created) setSelection([created]);
    },
    [applyAction],
  );

  const handleDeleteEntity = useCallback(
    async (id: string) => {
      const result = await applyAction({ kind: "delete", entity: id }, "delete entity");
      if (result) setSelection((current) => current.filter((entry) => entry !== id));
    },
    [applyAction, selectedId],
  );

  const handleDuplicateSelected = useCallback(async () => {
    if (!selectedId) return;
    const result = await applyAction({ kind: "duplicate", entity: selectedId }, "duplicate entity");
    const created = result?.touched?.[0];
    if (created) setSelection([created]);
  }, [applyAction, selectedId]);

  const handlePatchComponent = useCallback(
    async (entityId: string, component: string, value: Record<string, unknown>) => {
      const eligible = selection.filter((id) =>
        doc.entities.some((entity) => entity.id === id && entity.components[component] !== undefined),
      );
      if (eligible.length > 1) {
        try {
          const result = await api.engineApplyBatch(
            `edit ${component} on ${eligible.length} entities`,
            JSON.stringify(eligible.map((entity) => ({ kind: "patch_component", entity, component, value }))),
            activeScenePath || null,
          );
          setViewportTouched(result.edit?.touched ?? null);
          setScene(result.state);
          setSceneNotice(null);
          const actor = result.edit?.actor ?? "user";
          const summary = result.edit?.summary ?? `${eligible.length} entities updated`;
          setToast({ label: result.label, actor });
          log("info", actor, `${result.label} · ${summary}`);
        } catch (error: any) {
          report(error, `edit ${component} on the selection`);
        }
        return;
      }
      await applyAction(
        { kind: "patch_component", entity: entityId, component, value },
        `edit ${component}`,
      );
    },
    [activeScenePath, applyAction, doc.entities, log, report, selection],
  );

  const handleAddComponent = useCallback(
    async (entityId: string, component: string) => {
      // An empty payload is valid for every component whose fields are all optional; the
      // engine rejects it otherwise and the notice bar says which field is missing.
      await applyAction(
        { kind: "add_component", entity: entityId, component, value: {} },
        `add ${component}`,
      );
    },
    [applyAction],
  );

  const handleRemoveComponent = useCallback(
    async (entityId: string, component: string) => {
      await applyAction(
        { kind: "remove_component", entity: entityId, component },
        `remove ${component}`,
      );
    },
    [applyAction],
  );

  const handleRename = useCallback(
    async (entityId: string, name: string) => {
      await applyAction({ kind: "rename", entity: entityId, name }, "rename entity");
    },
    [applyAction],
  );

  const handleSetTags = useCallback(
    async (entityId: string, tags: string[]) => {
      await applyAction({ kind: "set_tags", entity: entityId, tags }, "set tags");
    },
    [applyAction],
  );

  const handleSetVisible = useCallback(
    async (entityId: string, visible: boolean) => {
      await applyAction(
        { kind: "set_visible", entity: entityId, visible },
        visible ? "show entity" : "hide entity",
      );
    },
    [applyAction],
  );

  const handleSetLocked = useCallback(
    async (entityId: string, locked: boolean) => {
      await applyAction(
        { kind: "set_locked", entity: entityId, locked },
        locked ? "lock entity" : "unlock entity",
      );
    },
    [applyAction],
  );

  const handleReparent = useCallback(
    async (entityId: string, parent: string | null) => {
      await applyAction({ kind: "reparent", entity: entityId, parent }, "reparent entity");
    },
    [applyAction],
  );

  const handleCreateOrganizerFolder = useCallback(
    async (parent: string | null) => {
      await applyAction(
        { kind: "create_organizer_folder", name: "New Folder", parent },
        "create Outliner folder",
      );
    },
    [applyAction],
  );

  const handleRenameOrganizerFolder = useCallback(
    async (folder: string, name: string) => {
      await applyAction(
        { kind: "rename_organizer_folder", folder, name },
        "rename Outliner folder",
      );
    },
    [applyAction],
  );

  const handleMoveOrganizerFolder = useCallback(
    async (folder: string, parent: string | null) => {
      await applyAction(
        { kind: "move_organizer_folder", folder, parent },
        "move Outliner folder",
      );
    },
    [applyAction],
  );

  const handleDeleteOrganizerFolder = useCallback(
    async (folder: string) => {
      await applyAction(
        { kind: "delete_organizer_folder", folder },
        "flatten Outliner folder",
      );
    },
    [applyAction],
  );

  const handleMoveEntityToOrganizerFolder = useCallback(
    async (entity: string, folder: string | null) => {
      await applyAction(
        { kind: "move_entity_to_organizer_folder", entity, folder },
        "move entity to Outliner folder",
      );
    },
    [applyAction],
  );

  const handleTransform = useCallback(
    async (id: string, transform: SceneTransform) => {
      await applyAction(
        {
          kind: "set_transform",
          entity: id,
          pos: transform.pos,
          rot: transform.rot,
          scale: transform.scale,
        },
        "move/rotate/scale entity",
      );
    },
    [applyAction],
  );

  /** Drop / Replace Object: route the asset to the component its kind belongs to. */
  const handleApplyAsset = useCallback(
    async (path: string) => {
      if (!selectedId) {
        setSceneNotice("Select a mesh in the viewport or Outliner first.");
        return;
      }
      const lower = path.toLowerCase();
      if (lower.endsWith(".shader.json") || lower.endsWith(".wgsl") || lower.endsWith(".glsl")) {
        await applyAction(
          { kind: "add_component", entity: selectedId, component: "ShaderRef", value: { shader: path } },
          "assign shader",
        );
        return;
      }
      if (lower.includes("/textures/")) {
        await applyAction(
          {
            kind: "patch_component",
            entity: selectedId,
            component: "MaterialOverride",
            value: { albedo: path },
          },
          "assign texture",
        );
        return;
      }
      await applyAction(
        { kind: "patch_component", entity: selectedId, component: "MeshRenderer", value: { mesh: path } },
        "assign mesh",
      );
    },
    [applyAction, selectedId],
  );

  const handleImportReplace = useCallback(async () => {
    if (!isGame) {
      setSceneNotice("Create a game before importing meshes.");
      return;
    }
    try {
      const { open } = await import("@tauri-apps/plugin-dialog");
      const picked = await open({
        multiple: false,
        filters: [
          { name: "3D / textures", extensions: ["glb", "gltf", "obj", "fbx", "png", "jpg", "jpeg", "tga", "exr"] },
        ],
      });
      if (!picked || Array.isArray(picked)) return;
      const fileName = picked.split(/[/\\]/).pop() || `import_${Date.now()}`;
      const ext = fileName.split(".").pop()?.toLowerCase() || "glb";
      const isTexture = ["png", "jpg", "jpeg", "tga", "exr", "hdr"].includes(ext);
      const dest = `${isTexture ? "assets/textures" : "assets/models"}/${fileName}`;
      await api.importWorkspaceFile(picked, dest);
      await handleApplyAsset(dest);
    } catch (error: any) {
      report(error, "import the file");
    }
  }, [handleApplyAsset, isGame, report]);

  const handleSelect = useCallback(
    (id: string | null, additive = false) => {
      setSelection((current) => {
        let next: string[];
        if (!id) next = [];
        else if (!additive) next = [id];
        else if (current.includes(id)) next = current.filter((entry) => entry !== id);
        else next = [...current, id];
        // Selection is engine state so the agent can act on "this one" (get_selection).
        if (isGame && activeScenePath) {
          void api.engineSetSelection(activeScenePath, next).catch(() => {});
        }
        return next;
      });
    },
    [activeScenePath, isGame],
  );

  useEffect(() => {
    const onKeyDown = (e: KeyboardEvent) => {
      const tag = (e.target as HTMLElement)?.tagName || "";
      const typing = ["INPUT", "TEXTAREA"].includes(tag);
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "s") {
        e.preventDefault();
        void handleSaveScene();
        return;
      }
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "z") {
        if (typing) return;
        e.preventDefault();
        if (e.shiftKey) void redoEdit();
        else void undoEdit();
        return;
      }
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "y") {
        if (typing) return;
        e.preventDefault();
        void redoEdit();
        return;
      }
      if ((e.ctrlKey || e.metaKey) && e.shiftKey && e.key.toLowerCase() === "p") {
        e.preventDefault();
        setPaletteMode("commands");
        setPaletteOpen((open) => !open);
        return;
      }
      if ((e.ctrlKey || e.metaKey) && !e.shiftKey && e.key.toLowerCase() === "p") {
        if (typing) return;
        e.preventDefault();
        setPaletteMode("assets");
        setPaletteOpen((open) => !open);
        return;
      }
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "j") {
        e.preventDefault();
        setIsDrawerCollapsed((collapsed) => !collapsed);
        return;
      }
      if (e.altKey && e.key.toLowerCase() === "p") {
        if (typing || !isGame) return;
        e.preventDefault();
        if (isPlaying) stopPlay();
        else void startPlay();
        return;
      }
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "d") {
        if (typing) return;
        e.preventDefault();
        void handleDuplicateSelected();
        return;
      }
      if (!typing && isPlaying && e.key.toLowerCase() === "g") {
        e.preventDefault();
        setGameView((value) => !value);
        return;
      }
      if (typing || isPlaying) return;
      if (e.key === "Delete" || e.key === "Backspace") {
        if (selectedId) {
          e.preventDefault();
          void handleDeleteEntity(selectedId);
        }
      } else if (e.key === "q" || e.key === "Q") {
        setGizmoMode("select");
      } else if (e.key === "w" || e.key === "W") {
        setGizmoMode("translate");
      } else if (e.key === "e" || e.key === "E") {
        setGizmoMode("rotate");
      } else if (e.key === "r" || e.key === "R") {
        setGizmoMode("scale");
      } else if (e.key === "x" || e.key === "X") {
        setGizmoSpace((space) => (space === "world" ? "local" : "world"));
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [handleDeleteEntity, handleDuplicateSelected, handleSaveScene, isGame, isPlaying, redoEdit, selectedId, startPlay, stopPlay, undoEdit]);

  /// One command list, shared by the palette and the toolbar, so a command reachable one
  /// way is reachable both ways (ENG-147).
  const commands = useMemo<PaletteCommand[]>(
    () => [
      { id: "save", group: "File", label: "Save Scene", hint: "Ctrl+S", run: () => void handleSaveScene() },
      { id: "save-all", group: "File", label: "Save All", run: () => void api.engineSaveAll().catch((e: any) => report(e, "save all")) },
      { id: "reload", group: "File", label: "Reload Engine Status", run: () => void reload() },
      { id: "undo", group: "Edit", label: scene?.undo_label ? `Undo ${scene.undo_label}` : "Undo", hint: "Ctrl+Z", disabled: !scene?.can_undo, run: () => void undoEdit() },
      { id: "redo", group: "Edit", label: "Redo", hint: "Ctrl+Y", disabled: !scene?.can_redo, run: () => void redoEdit() },
      { id: "duplicate", group: "Edit", label: "Duplicate Selection", hint: "Ctrl+D", disabled: !selectedId, run: () => void handleDuplicateSelected() },
      { id: "delete", group: "Edit", label: "Delete Selection", hint: "Del", disabled: !selectedId, run: () => selectedId && void handleDeleteEntity(selectedId) },
      ...templates.map((template) => ({
        id: `add-${template.name}`,
        group: "Add",
        label: `Add ${template.label}`,
        run: () => void handleAddEntity(template.name),
      })),
      ...presets.map((preset) => ({
        id: `weather-${preset.id}`,
        group: "Weather",
        label: `Weather: ${preset.label}`,
        run: () => void handleWeather(preset.id),
      })),
      { id: "play", group: "Play", label: isPlaying ? "Stop" : "Play", hint: "Alt+P", disabled: !isGame, run: () => (isPlaying ? stopPlay() : void startPlay()) },
      { id: "tab-scene", group: "View", label: "Go to Scene editor", run: () => setEditorTab("scene") },
      { id: "tab-hud", group: "View", label: "Go to HUD editor", disabled: !isGame, run: () => setEditorTab("hud") },
      { id: "wireframe", group: "View", label: wireframe ? "Lit shading" : "Wireframe shading", run: () => setShadingMode(wireframe ? "lit" : "wireframe") },
      { id: "grid", group: "View", label: showFlags.grid ? "Hide grid" : "Show grid", run: () => setShowFlags((flags) => ({ ...flags, grid: !flags.grid })) },
      { id: "log", group: "View", label: logOpen ? "Hide Output Log" : "Show Output Log", run: () => toggleDrawerTab("output") },
      { id: "drawer", group: "View", label: isDrawerCollapsed ? "Show Content Drawer" : "Hide Content Drawer", hint: "Ctrl+J", run: () => toggleDrawerTab("content") },
      {
        id: "check",
        group: "Project",
        label: "Check content (gates)",
        disabled: !isGame,
        run: () => {
          void api
            .engineCheckContent(false)
            .then((report) => {
              const blockers = report.findings.filter((finding) => finding.level === "blocker");
              setSceneNotice(
                blockers.length === 0
                  ? `Content check passed — ${report.findings.length} note(s).`
                  : `${blockers.length} blocker(s): ${blockers.map((finding) => finding.message).join(" · ")}`,
              );
            })
            .catch((error: any) => report(error, "check the content"));
        },
      },
    ],
    [
      handleAddEntity,
      handleDeleteEntity,
      handleDuplicateSelected,
      handleSaveScene,
      handleWeather,
      isDrawerCollapsed,
      isGame,
      logOpen,
      isPlaying,
      presets,
      redoEdit,
      reload,
      report,
      toggleDrawerTab,
      scene?.can_redo,
      scene?.can_undo,
      scene?.undo_label,
      selectedId,
      showFlags.grid,
      startPlay,
      stopPlay,
      templates,
      undoEdit,
      wireframe,
    ],
  );

  const sceneName = activeScenePath.split("/").pop() || doc.name;
  const dirty = scene?.dirty ?? false;
  const weatherId = String(doc.settings.weather || "clear");
  const assetCommands = useMemo<PaletteCommand[]>(() => {
    const raw: { key: string; value: PaletteCommand }[] = [
      ...(status?.game?.scenes ?? []).map((path) => ({ key: path, value: {
        id: `scene-${path}`,
        group: "Scene",
        label: path.split("/").pop() ?? path,
        hint: path,
        run: () => { rememberQuickOpen(path); void handleSelectScene(path); },
      }})),
      ...(status?.game?.hud_scene ? [{
        key: status.game.hud_scene,
        value: {
          id: `hud-${status.game.hud_scene}`,
          group: "HUD",
          label: status.game.hud_scene.split("/").pop() ?? status.game.hud_scene,
          hint: status.game.hud_scene,
          run: () => { rememberQuickOpen(status.game!.hud_scene!); setEditorTab("hud"); },
        },
      }] : []),
      ...paletteAssets.map((asset) => ({ key: asset.path, value: {
        id: `asset-${asset.id}`,
        group: asset.kind,
        label: asset.path.split("/").pop() ?? asset.path,
        hint: asset.path,
        run: () => {
          rememberQuickOpen(asset.path);
          if (asset.path.endsWith(".hud.json")) setEditorTab("hud");
          else requestOpenWorkspaceFile(asset.path, 1);
        },
      }})),
    ];
    const seen = new Set<string>();
    const unique = raw.filter((item) => {
      if (seen.has(item.key)) return false;
      seen.add(item.key);
      return true;
    });
    return rankQuickOpen(unique, recentQuickOpen);
  }, [handleSelectScene, paletteAssets, recentQuickOpen, rememberQuickOpen, status?.game?.hud_scene, status?.game?.scenes]);

  useEffect(() => {
    if (!paletteAssetsLoaded) return;
    const existing = new Set(assetCommands.map((command) => command.hint).filter((path): path is string => !!path));
    setRecentQuickOpen((current) => {
      const next = evictMissingRecent(current, existing);
      if (next.length === current.length && next.every((value, index) => value === current[index])) return current;
      localStorage.setItem(recentStorageKey, JSON.stringify(next));
      return next;
    });
  }, [assetCommands, paletteAssetsLoaded, recentStorageKey]);

  return (
    <div className={`engine-view${viewportMaximized ? " viewport-maximized" : ""}`}>
      <EngineCommandPalette
        open={paletteOpen}
        commands={paletteMode === "commands" ? commands : assetCommands}
        onClose={() => setPaletteOpen(false)}
        label={paletteMode === "commands" ? "Engine command palette" : "Scene and asset palette"}
        placeholder={paletteMode === "commands" ? "Type a command…" : "Find a scene or asset…"}
      />
      {/* ── Unreal Engine Style Toolbar (Top) ───────────────────────────── */}
      <header className="engine-toolbar">
        <div className="toolbar-section left">
          <span className="engine-stage" aria-hidden="true">
            <IconEngine size={15} />
          </span>
          <span className="engine-heading">Game Engine</span>

          <div className="engine-scene-selector" title={activeScenePath || "No scene"}>
            <span className={`scene-dot${dirty ? " dirty" : isGame ? " active" : ""}`} />
            <span className="scene-name-label">{isGame ? sceneName : "Empty"}</span>
          </div>
          {!isGame ? (
            <button type="button" className="engine-save-pill-btn" onClick={() => void handleNewGame()} title="Scaffold Main, HUD, and Level 1">
              <span>New Game</span>
            </button>
          ) : null}

          <button
            type="button"
            className={`engine-save-pill-btn${savedFeedback ? " saved" : ""}`}
            onClick={() => void handleSaveScene()}
            title="Save Scene (Ctrl+S)"
          >
            {savedFeedback ? <IconBadgeCheck size={12} /> : null}
            <span>{savedFeedback ? "Saved!" : dirty ? "Save *" : "Saved"}</span>
          </button>
        </div>

        {/* Center: Simulator Transport & Gizmo Controls */}
        <div className="toolbar-section center">
          <div className="engine-transport-group" role="toolbar" aria-label="Play controls">
            <button
              type="button"
              className={`transport-btn play${isPlaying ? " active" : ""}`}
              onClick={() => {
                if (isPlaying) togglePlayPause();
                else void startPlay();
              }}
              disabled={!isGame}
              title={isPlaying ? (playPaused ? "Resume" : "Pause") : "Play (Main = full game, Level = that map + HUD)"}
            >
              {isPlaying && !playPaused ? <IconPause size={12} /> : <IconPlay size={12} />}
              <span>{isPlaying ? (playPaused ? "Resume" : "Pause") : "Play"}</span>
            </button>
            <button
              type="button"
              className="transport-btn"
              onClick={() => setPlayStepToken((value) => value + 1)}
              disabled={!isPlaying || !playPaused}
              title="Step one frame"
            >
              Step
            </button>
            <button
              type="button"
              className="transport-btn stop"
              onClick={stopPlay}
              disabled={!isPlaying}
              title="Stop Simulation"
            >
              <IconStop size={11} />
            </button>
            {isPlaying ? (
              <div className="spawn-entity-wrap engine-play-options">
                <button
                  type="button"
                  className={`transport-btn${playOptionsOpen ? " active" : ""}`}
                  onClick={() => setPlayOptionsOpen((open) => !open)}
                  aria-expanded={playOptionsOpen}
                  title="Play options and live simulation status"
                >
                  <span>Options</span>
                  {playStats ? <span className="engine-play-options-summary">{Math.round(playStats.fps)} fps</span> : null}
                  <span aria-hidden="true">⌄</span>
                </button>
                {playOptionsOpen ? (
                  <div className="engine-dropdown-menu engine-play-options-menu m-fade" role="group" aria-label="Play options">
                    <div className="engine-play-options-actions">
                      <button type="button" className="dropdown-item" onClick={() => setPlayRestartToken((value) => value + 1)}>Restart simulation</button>
                      <button type="button" className="dropdown-item" disabled={!playPaused} onClick={() => setPlayStepToken((value) => value + 1)}>Step one frame</button>
                    </div>
                    <label className="engine-menu-field">
                      <span>Simulation speed</span>
                      <select className="engine-capability-select" value={playTimeScale} onChange={(event) => setPlayTimeScale(Number(event.target.value))}>
                        <option value={0.25}>0.25×</option>
                        <option value={0.5}>0.5×</option>
                        <option value={1}>1×</option>
                        <option value={2}>2×</option>
                      </select>
                    </label>
                    <div className="engine-menu-separator" />
                    <button type="button" className={`dropdown-item${gameView ? " active" : ""}`} onClick={() => setGameView((value) => !value)} aria-pressed={gameView}>Game View <kbd>G</kbd></button>
                    <button type="button" className={`dropdown-item${playEjected ? " active" : ""}`} onClick={() => setPlayEjected((value) => !value)} aria-pressed={playEjected}>{playEjected ? "Possess game camera" : "Eject to editor camera"}</button>
                    <button type="button" className={`dropdown-item${pauseOnScriptError ? " active" : ""}`} onClick={() => setPauseOnScriptError((value) => !value)} aria-pressed={pauseOnScriptError}>Break on script error</button>
                    {playStats ? (
                      <div className="engine-play-metrics" role="status">
                        <span>{Math.round(playStats.fps)} fps</span>
                        <span>{playStats.frameMs.toFixed(1)} ms</span>
                        <span>{playStats.entities} entities</span>
                        <span>{playStats.contacts} contacts</span>
                        <span>{playStats.drawCalls} draws</span>
                        <span>{playStats.scripts} scripts</span>
                        {playStats.scriptFaults > 0 ? <span className="error">{playStats.scriptFaults} script errors</span> : null}
                      </div>
                    ) : <div className="engine-menu-empty">Waiting for the first simulation frame…</div>}
                  </div>
                ) : null}
              </div>
            ) : null}
            <button
              type="button"
              className="transport-btn"
              onClick={() => setPlayRestartToken((value) => value + 1)}
              disabled={!isPlaying}
              title="Restart simulation from the authored snapshot"
            >
              Restart
            </button>
            <select
              className="transport-select"
              aria-label="Play speed"
              value={playTimeScale}
              disabled={!isPlaying}
              onChange={(event) => setPlayTimeScale(Number(event.target.value))}
            >
              <option value={0.25}>0.25×</option>
              <option value={0.5}>0.5×</option>
              <option value={1}>1×</option>
              <option value={2}>2×</option>
            </select>
            <button
              type="button"
              className={`transport-btn${gameView ? " active" : ""}`}
              onClick={() => setGameView((value) => !value)}
              disabled={!isPlaying}
              title="Game View (G)"
              aria-pressed={gameView}
            >
              G
            </button>
            <button
              type="button"
              className={`transport-btn${playEjected ? " active" : ""}`}
              onClick={() => setPlayEjected((value) => !value)}
              disabled={!isPlaying}
              title="Eject to editor camera while simulation keeps running"
            >
              {playEjected ? "Possess" : "Eject"}
            </button>
            <button
              type="button"
              className={`transport-btn${pauseOnScriptError ? " active" : ""}`}
              onClick={() => setPauseOnScriptError((value) => !value)}
              title="Pause the simulation on the frame a gameplay script faults"
              aria-pressed={pauseOnScriptError}
            >
              Break
            </button>
            {playStats ? (
              <span className="engine-play-stats" role="status">
                {Math.round(playStats.fps)} fps · {playStats.frameMs.toFixed(1)} ms ·{" "}
                {playStats.entities} entities · {playStats.contacts} contacts ·{" "}
                {playStats.drawCalls} draws
                {playStats.scripts > 0 ? ` · ${playStats.scripts} scripts` : ""}
                {playStats.scriptFaults > 0 ? ` · ${playStats.scriptFaults} script errors` : ""}
              </span>
            ) : null}
          </div>

          <div className="toolbar-divider" />

          <div className="engine-gizmo-group">
            <button type="button" className={`gizmo-tool-btn${gizmoMode === "select" ? " active" : ""}`} onClick={() => setGizmoMode("select")} title="Select (Q)">↖</button>
            <button type="button" className={`gizmo-tool-btn${gizmoMode === "translate" ? " active" : ""}`} onClick={() => setGizmoMode("translate")} title="Translate / Move (W)">✥</button>
            <button type="button" className={`gizmo-tool-btn${gizmoMode === "rotate" ? " active" : ""}`} onClick={() => setGizmoMode("rotate")} title="Rotate (E)">↻</button>
            <button type="button" className={`gizmo-tool-btn${gizmoMode === "scale" ? " active" : ""}`} onClick={() => setGizmoMode("scale")} title="Scale (R)">⤢</button>
          </div>

          <div className="engine-gizmo-group" title="Gizmo space">
            <button type="button" className={`gizmo-tool-btn wide${gizmoSpace === "world" ? " active" : ""}`} onClick={() => setGizmoSpace("world")} title="World space (X)">Wld</button>
            <button type="button" className={`gizmo-tool-btn wide${gizmoSpace === "local" ? " active" : ""}`} onClick={() => setGizmoSpace("local")} title="Local space (X)">Loc</button>
          </div>

          <div className="engine-gizmo-group" title="Grid snap">
            {([null, 10, 1, 0.1] as const).map((step) => (
              <button
                key={String(step)}
                type="button"
                className={`gizmo-tool-btn wide${snap === step ? " active" : ""}`}
                onClick={() => setSnap(step)}
                title={step == null ? "Snap off" : `Snap ${step}`}
              >
                {step == null ? "Off" : step}
              </button>
            ))}
          </div>

          <div className="engine-gizmo-group">
            <button
              type="button"
              className="gizmo-tool-btn wide"
              onClick={() => void handleDuplicateSelected()}
              disabled={!selectedId || isPlaying}
              title="Duplicate (Ctrl+D)"
            >
              Dup
            </button>
            <button
              type="button"
              className="gizmo-tool-btn wide"
              onClick={() => selectedId && void handleDeleteEntity(selectedId)}
              disabled={!selectedId || isPlaying}
              title="Delete (Del)"
            >
              Del
            </button>
          </div>

          <div className="engine-gizmo-group" title="Undo / redo — user and agent edits share one stack">
            <button
              type="button"
              className="gizmo-tool-btn wide"
              onClick={() => void undoEdit()}
              disabled={!scene?.can_undo || isPlaying}
              title={scene?.undo_label ? `Undo ${scene.undo_label} (Ctrl+Z)` : "Undo (Ctrl+Z)"}
            >
              Undo
            </button>
            <button
              type="button"
              className="gizmo-tool-btn wide"
              onClick={() => void redoEdit()}
              disabled={!scene?.can_redo || isPlaying}
              title={scene?.redo_label ? `Redo ${scene.redo_label} (Ctrl+Y)` : "Redo (Ctrl+Y)"}
            >
              Redo
            </button>
          </div>

          <div className="toolbar-divider" />

          <div className="engine-shading-group">
            {(["lit", "unlit", "wireframe", "detail_lighting", "lighting_only", "collision"] as const).map((mode) => (
              <button key={mode} type="button" className={`shading-btn${shadingMode === mode ? " active" : ""}`} onClick={() => setShadingMode(mode)} title={`${mode.replaceAll("_", " ")} shading`}>
                {mode.replaceAll("_", " ")}
              </button>
            ))}
          </div>

          <div className="spawn-entity-wrap">
            <button
              type="button"
              className={`engine-tool-action-btn${showMenu ? " active" : ""}`}
              onClick={() => setShowMenu((open) => !open)}
              title="Show flags"
            >
              <span>Show</span>
            </button>
            {showMenu ? (
              <div className="engine-dropdown-menu m-fade">
                {(["grid", "icons", "bounds", "colliders"] as const).map((flag) => (
                  <button
                    key={flag}
                    type="button"
                    className="dropdown-item"
                    onClick={() => setShowFlags((flags) => ({ ...flags, [flag]: !flags[flag] }))}
                  >
                    {showFlags[flag] ? "☑" : "☐"} {flag}
                  </button>
                ))}
              </div>
            ) : null}
          </div>

          <label className="engine-camera-speed" title="Fly camera speed">
            <span>Cam</span>
            <input
              type="range"
              min={0.25}
              max={4}
              step={0.25}
              value={cameraSpeed}
              onChange={(e) => setCameraSpeed(Number(e.target.value))}
              aria-label="Camera speed"
            />
          </label>

          <select
            className="engine-camera-select"
            value={cameraMode}
            onChange={(e) => setCameraMode(e.target.value as any)}
            title="Camera Perspective Bookmark"
          >
            <option value="perspective">Perspective</option>
            <option value="top">Top View</option>
            <option value="bottom">Bottom View</option>
            <option value="front">Front View</option>
            <option value="back">Back View</option>
            <option value="left">Left View</option>
            <option value="right">Right View</option>
          </select>

          <label className="engine-camera-speed" title="Viewport field of view">
            <span>FOV</span>
            <input type="range" min={25} max={110} value={viewportFov} onChange={(event) => setViewportFov(Number(event.target.value))} aria-label="Viewport field of view" />
          </label>

          <select className="engine-camera-select" value={screenPercentage} onChange={(event) => setScreenPercentage(Number(event.target.value))} title="Screen percentage">
            <option value={50}>50%</option>
            <option value={75}>75%</option>
            <option value={100}>100%</option>
          </select>
          <button type="button" className={`engine-tool-action-btn${viewportMaximized ? " active" : ""}`} onClick={() => setViewportMaximized((value) => !value)}>
            {viewportMaximized ? "Restore" : "Maximise"}
          </button>

          <select
            className="engine-camera-select"
            value={agentMode}
            onChange={(e) => {
              const next = e.target.value;
              setAgentMode(next);
              void api.setEnginePermissionMode(next).catch((error: any) => {
                report(error, "change the agent permission mode");
              });
            }}
            title="How much the AI may change without asking. Every change is undoable either way."
          >
            <option value="ask">AI: ask first</option>
            <option value="auto">AI: auto (ask before deletes)</option>
            <option value="autonomous">AI: autonomous</option>
          </select>

          <div className="spawn-entity-wrap">
            <button
              type="button"
              className={`engine-tool-action-btn${capabilityMenu ? " active" : ""}`}
              onClick={() => {
                const opening = !capabilityMenu;
                setCapabilityMenu(opening);
                if (opening) {
                  void api
                    .engineAgentCapabilities()
                    .then(setCapabilities)
                    .catch((error: any) => report(error, "read agent capabilities"));
                }
              }}
              disabled={!isGame}
              aria-expanded={capabilityMenu}
              title="What the agent may do to this project. Stored in Bhippi.game.toml, so it travels with the game."
            >
              <IconAlert size={12} />
              <span>Agent</span>
            </button>
            {capabilityMenu ? (
              <div className="engine-dropdown-menu right m-fade" role="group" aria-label="Agent capabilities">
                {capabilities.length === 0 ? (
                  <div className="engine-menu-empty">No capabilities to show.</div>
                ) : null}
                {capabilities.map((row) => (
                  <label key={row.capability} className="engine-capability-row" title={row.doc}>
                    <span className="engine-capability-name">
                      {row.capability.replace(/_/g, " ")}
                      {row.is_default ? "" : " *"}
                    </span>
                    <select
                      className="engine-capability-select"
                      value={row.decision}
                      onChange={(e) => {
                        void api
                          .engineSetAgentCapability(row.capability, e.target.value)
                          .then(setCapabilities)
                          .catch((error: any) => report(error, `set ${row.capability}`));
                      }}
                    >
                      <option value="allow">Allow</option>
                      <option value="ask">Ask first</option>
                      <option value="deny">Deny</option>
                    </select>
                  </label>
                ))}
              </div>
            ) : null}
          </div>

          <div className="spawn-entity-wrap">
            <button
              type="button"
              className={`engine-tool-action-btn${weatherMenu ? " active" : ""}`}
              onClick={() => setWeatherMenu((open) => !open)}
              disabled={!isGame}
              title="Weather templates (UltraSky-style)"
            >
              <IconSun size={12} />
              <span>{weatherId}</span>
            </button>
            {weatherMenu ? (
              <div className="engine-dropdown-menu right m-fade">
                {presets.map((preset) => (
                  <button
                    key={preset.id}
                    type="button"
                    className="dropdown-item"
                    onClick={() => void handleWeather(preset.id)}
                  >
                    {preset.label}
                  </button>
                ))}
              </div>
            ) : null}
          </div>
        </div>

        {/* Right Actions */}
        <div className="toolbar-section right">
          <div className="spawn-entity-wrap engine-primary-add">
            <button
              type="button"
              className="engine-tool-action-btn primary"
              onClick={() => setShowAddMenu(!showAddMenu)}
              disabled={!isGame}
              title="Add Primitive to 3D Scene"
            >
              <IconPlus size={12} />
              <span>Add</span>
            </button>
            {showAddMenu ? (
              <div className="engine-dropdown-menu right m-fade">
                {templates.map((template) => (
                  <button
                    key={template.name}
                    type="button"
                    className="dropdown-item"
                    onClick={() => void handleAddEntity(template.name)}
                  >
                    {TEMPLATE_ICONS[template.name] ?? <IconLayers size={12} />} {template.label}
                  </button>
                ))}
              </div>
            ) : null}
          </div>

          <div className="engine-gizmo-group" title="Scene or HUD">
            <button
              type="button"
              className={`gizmo-tool-btn wide${editorTab === "scene" ? " active" : ""}`}
              onClick={() => setEditorTab("scene")}
            >
              Scene
            </button>
            <button
              type="button"
              className={`gizmo-tool-btn wide${editorTab === "hud" ? " active" : ""}`}
              onClick={() => setEditorTab("hud")}
              disabled={!isGame}
              title="Edit the HUD document — text, buttons, bars"
            >
              HUD
            </button>
          </div>

          <button
            type="button"
            className={`engine-tool-action-btn${!isDrawerCollapsed ? " active" : ""}`}
            onClick={() => setIsDrawerCollapsed(!isDrawerCollapsed)}
            title="Toggle Unreal-style Content Drawer"
          >
            <IconGrid size={12} />
            <span>Content Drawer</span>
          </button>

          <button
            type="button"
            className={`engine-tool-action-btn${logOpen ? " active" : ""}`}
            onClick={() => toggleDrawerTab("output")}
            title="Output Log — every applied change, with its actor"
          >
            <span>Log</span>
          </button>

          <button
            type="button"
            className="engine-tool"
            onClick={() => void reload()}
            disabled={loading}
            title="Reload Engine Status"
          >
            <IconRefresh size={13} />
          </button>

          <div className="spawn-entity-wrap engine-simplified-action">
            <button
              type="button"
              className={`engine-tool-action-btn${capabilityMenu ? " active" : ""}`}
              onClick={() => {
                const opening = !capabilityMenu;
                setCapabilityMenu(opening);
                setToolbarMoreOpen(false);
                if (opening) {
                  void api.engineAgentCapabilities().then(setCapabilities).catch((error: any) => report(error, "read agent capabilities"));
                }
              }}
              disabled={!isGame}
              aria-expanded={capabilityMenu}
              title="AI permissions and autonomy for this game"
            >
              <span className={`engine-ai-status-dot ${agentMode}`} aria-hidden="true" />
              <span>AI</span>
            </button>
            {capabilityMenu ? (
              <div className="engine-dropdown-menu right engine-ai-menu m-fade" role="group" aria-label="AI permissions">
                <label className="engine-menu-field">
                  <span>Change policy</span>
                  <select
                    className="engine-capability-select"
                    value={agentMode}
                    onChange={(event) => {
                      const next = event.target.value;
                      setAgentMode(next);
                      void api.setEnginePermissionMode(next).catch((error: any) => report(error, "change the agent permission mode"));
                    }}
                  >
                    <option value="ask">Ask first</option>
                    <option value="auto">Auto · ask before deletes</option>
                    <option value="autonomous">Autonomous</option>
                  </select>
                </label>
                <div className="engine-menu-section-label">Capabilities</div>
                {capabilities.length === 0 ? <div className="engine-menu-empty">No capabilities to show.</div> : null}
                {capabilities.map((row) => (
                  <label key={row.capability} className="engine-capability-row" title={row.doc}>
                    <span className="engine-capability-name">{row.capability.replace(/_/g, " ")}{row.is_default ? "" : " *"}</span>
                    <select
                      className="engine-capability-select"
                      value={row.decision}
                      onChange={(event) => {
                        void api.engineSetAgentCapability(row.capability, event.target.value).then(setCapabilities).catch((error: any) => report(error, `set ${row.capability}`));
                      }}
                    >
                      <option value="allow">Allow</option>
                      <option value="ask">Ask</option>
                      <option value="deny">Deny</option>
                    </select>
                  </label>
                ))}
              </div>
            ) : null}
          </div>

          <div className="spawn-entity-wrap engine-simplified-action">
            <button
              type="button"
              className={`engine-tool-action-btn${toolbarMoreOpen ? " active" : ""}`}
              onClick={() => {
                setToolbarMoreOpen((open) => !open);
                setCapabilityMenu(false);
              }}
              aria-expanded={toolbarMoreOpen}
              title="More engine commands"
            >
              <span>More</span>
              <span aria-hidden="true">⌄</span>
            </button>
            {toolbarMoreOpen ? (
              <div className="engine-dropdown-menu right engine-more-menu m-fade">
                <button type="button" className="dropdown-item" onClick={() => { setPaletteMode("commands"); setPaletteOpen(true); setToolbarMoreOpen(false); }}>Command palette <kbd>Ctrl Shift P</kbd></button>
                <button type="button" className="dropdown-item" onClick={() => { setPaletteMode("assets"); setPaletteOpen(true); setToolbarMoreOpen(false); }}>Quick open <kbd>Ctrl P</kbd></button>
                <div className="engine-menu-separator" />
                <button type="button" className="dropdown-item" disabled={!scene?.can_undo || isPlaying} onClick={() => { void undoEdit(); setToolbarMoreOpen(false); }}>Undo{scene?.undo_label ? ` ${scene.undo_label}` : ""}</button>
                <button type="button" className="dropdown-item" disabled={!scene?.can_redo || isPlaying} onClick={() => { void redoEdit(); setToolbarMoreOpen(false); }}>Redo{scene?.redo_label ? ` ${scene.redo_label}` : ""}</button>
                <button type="button" className="dropdown-item" disabled={!selectedId || isPlaying} onClick={() => { void handleDuplicateSelected(); setToolbarMoreOpen(false); }}>Duplicate selection</button>
                <button type="button" className="dropdown-item danger" disabled={!selectedId || isPlaying} onClick={() => { if (selectedId) void handleDeleteEntity(selectedId); setToolbarMoreOpen(false); }}>Delete selection</button>
                <div className="engine-menu-separator" />
                <button type="button" className="dropdown-item" onClick={() => { toggleDrawerTab("content"); setToolbarMoreOpen(false); }}>{!isDrawerCollapsed && drawerTab === "content" ? "Close Content" : "Open Content"}</button>
                <button type="button" className="dropdown-item" onClick={() => { toggleDrawerTab("output"); setToolbarMoreOpen(false); }}>{logOpen ? "Close Output" : "Open Output"}</button>
                <button type="button" className="dropdown-item" onClick={() => { setViewportMaximized((value) => !value); setToolbarMoreOpen(false); }}>{viewportMaximized ? "Restore workspace" : "Maximise viewport"}</button>
                <button type="button" className="dropdown-item" onClick={() => { void reload(); setToolbarMoreOpen(false); }}>Reload engine</button>
              </div>
            ) : null}
          </div>
        </div>
      </header>

      {/* ── Conflict bar: the file changed under unsaved work (ENG-108) ──── */}
      {scene?.recovery_available && !scene.dirty ? (
        <div className="engine-notice-bar m-fade">
          <IconAlert size={13} className="notice-icon" />
          <span className="notice-text">
            Unsaved work from an interrupted session is available for {sceneName}.
          </span>
          <button type="button" className="notice-action-btn" onClick={() => void recoverScene()}>
            Recover
          </button>
          <button type="button" className="notice-action-btn" onClick={() => void handleTakeDisk()}>
            Discard
          </button>
        </div>
      ) : null}
      {scene?.disk_conflict ? (
        <div className="engine-notice-bar m-fade">
          <IconAlert size={13} className="notice-icon" />
          <span className="notice-text">
            {sceneName} changed on disk while you have unsaved edits. Saving keeps yours.
          </span>
          <button type="button" className="notice-action-btn" onClick={() => void handleSaveScene()} title="Overwrite the file with the version you are editing">
            Keep mine
          </button>
          <button type="button" className="notice-action-btn" onClick={() => void handleTakeDisk()} title="Discard your edits and re-read the file">
            Take disk
          </button>
          <button type="button" className="notice-action-btn" onClick={() => void handleShowDiff()} title="Compare your live edits with the file on disk">
            Diff
          </button>
        </div>
      ) : null}

      {sceneDiff ? (
        <div className="engine-diff-backdrop" role="presentation" onClick={() => setSceneDiff(null)}>
          <section className="engine-diff-dialog" role="dialog" aria-modal="true" aria-label="Scene conflict diff" onClick={(event) => event.stopPropagation()}>
            <header><strong>{sceneDiff.scene_path}</strong><button type="button" onClick={() => setSceneDiff(null)}>Close</button></header>
            <div className="engine-diff-columns">
              <div><h3>Your unsaved scene</h3><pre>{sceneDiff.mine_json}</pre></div>
              <div><h3>File on disk</h3><pre>{sceneDiff.disk_json}</pre></div>
            </div>
          </section>
        </div>
      ) : null}

      {/* ── Non-blocking Scene Notice Bar ────────────────────────────────── */}
      {sceneNotice ? (
        <div className="engine-notice-bar m-fade">
          <IconAlert size={13} className="notice-icon" />
          <span className="notice-text">{sceneNotice}</span>
          <button type="button" className="notice-close-btn" onClick={() => setSceneNotice(null)}>
            ✕
          </button>
        </div>
      ) : null}

      {toast ? (
        <div className="engine-toast m-fade" role="status">
          <span className={`engine-toast-actor ${toast.actor}`}>
            {toast.actor === "agent" ? "Agent" : "You"}
          </span>
          <span className="engine-toast-label">{toast.label}</span>
          <button
            type="button"
            className="notice-action-btn"
            onClick={() => {
              setToast(null);
              void undoEdit();
            }}
            disabled={!scene?.can_undo}
          >
            Undo
          </button>
          <button type="button" className="notice-close-btn" onClick={() => setToast(null)}>
            ✕
          </button>
        </div>
      ) : null}

      {/* ── Main Unreal Engine Editor Layout ────────────────────────────── */}
      <div className="engine-workspace-body">
        <div className={`engine-viewport-row narrow-focus-${narrowFocus}`}>
          <nav className="engine-narrow-focus" aria-label="Focused engine panel">
            {(["world", "viewport", "details"] as const).map((panel) => (
              <button
                key={panel}
                type="button"
                className={narrowFocus === panel ? "active" : ""}
                aria-pressed={narrowFocus === panel}
                onClick={() => setNarrowFocus(panel)}
              >
                {panel === "world" ? "World" : panel === "viewport" ? "Viewport" : "Details"}
              </button>
            ))}
          </nav>
          <nav className="engine-mode-rail" aria-label="Engine editor mode">
            <button type="button" className={editorTab === "scene" ? "active" : ""} onClick={() => setEditorTab("scene")} title="Scene editor"><IconLayers size={15} /><span>Scene</span></button>
            <button type="button" className={editorTab === "hud" ? "active" : ""} onClick={() => setEditorTab("hud")} disabled={!isGame} title="HUD editor"><IconGrid size={15} /><span>HUD</span></button>
          </nav>

          {editorTab === "hud" ? (
            <EngineHudEditor refreshToken={refreshToken} onNotice={setSceneNotice} />
          ) : (
            <>
              <EngineHierarchy
                entities={doc.entities}
                folders={doc.editor.folders}
                entityFolders={doc.editor.entity_folders}
                selection={selection}
                templates={templates}
                onSelect={(id, additive) => handleSelect(id, additive)}
                onAddEntity={(template) => void handleAddEntity(template)}
                onDeleteEntity={(id) => void handleDeleteEntity(id)}
                onSetVisible={(id, visible) => void handleSetVisible(id, visible)}
                onSetLocked={(id, locked) => void handleSetLocked(id, locked)}
                onReparent={(id, parent) => void handleReparent(id, parent)}
                onCreateFolder={(parent) => void handleCreateOrganizerFolder(parent)}
                onRenameFolder={(folder, name) => void handleRenameOrganizerFolder(folder, name)}
                onMoveFolder={(folder, parent) => void handleMoveOrganizerFolder(folder, parent)}
                onDeleteFolder={(folder) => void handleDeleteOrganizerFolder(folder)}
                onMoveEntityToFolder={(entity, folder) => void handleMoveEntityToOrganizerFolder(entity, folder)}
                onFocus={(id) => handleSelect(id)}
              />

              <main className="engine-viewport-center">
                <div className="engine-viewport-toolbar" role="toolbar" aria-label="Viewport tools">
                  <div className="engine-gizmo-group">
                    <button type="button" className={`gizmo-tool-btn${gizmoMode === "select" ? " active" : ""}`} onClick={() => setGizmoMode("select")} title="Select (Q)">↖</button>
                    <button type="button" className={`gizmo-tool-btn${gizmoMode === "translate" ? " active" : ""}`} onClick={() => setGizmoMode("translate")} title="Move (W)">✥</button>
                    <button type="button" className={`gizmo-tool-btn${gizmoMode === "rotate" ? " active" : ""}`} onClick={() => setGizmoMode("rotate")} title="Rotate (E)">↻</button>
                    <button type="button" className={`gizmo-tool-btn${gizmoMode === "scale" ? " active" : ""}`} onClick={() => setGizmoMode("scale")} title="Scale (R)">⤢</button>
                  </div>
                  <button type="button" className="engine-context-btn" onClick={() => setGizmoSpace((value) => value === "world" ? "local" : "world")} title="Toggle world/local space (X)">{gizmoSpace === "world" ? "World" : "Local"}</button>
                  <select className="engine-context-select" value={snap ?? "off"} onChange={(event) => setSnap(event.target.value === "off" ? null : Number(event.target.value))} aria-label="Grid snap">
                    <option value="off">Snap off</option><option value="0.1">0.1</option><option value="1">1</option><option value="10">10</option>
                  </select>
                  <select className="engine-context-select" value={shadingMode} onChange={(event) => setShadingMode(event.target.value as typeof shadingMode)} aria-label="Viewport shading">
                    <option value="lit">Lit</option><option value="unlit">Unlit</option><option value="wireframe">Wireframe</option><option value="detail_lighting">Detail lighting</option><option value="lighting_only">Lighting only</option><option value="collision">Collision</option>
                  </select>
                  <select className="engine-context-select" value={cameraMode} onChange={(event) => setCameraMode(event.target.value as typeof cameraMode)} aria-label="Viewport camera">
                    <option value="perspective">Perspective</option><option value="top">Top</option><option value="bottom">Bottom</option><option value="front">Front</option><option value="back">Back</option><option value="left">Left</option><option value="right">Right</option>
                  </select>
                  <div className="spawn-entity-wrap">
                    <button type="button" className={`engine-context-btn${showMenu ? " active" : ""}`} onClick={() => setShowMenu((open) => !open)}>Show</button>
                    {showMenu ? <div className="engine-dropdown-menu m-fade">{(["grid", "icons", "bounds", "colliders"] as const).map((flag) => <button key={flag} type="button" className="dropdown-item" onClick={() => setShowFlags((flags) => ({ ...flags, [flag]: !flags[flag] }))}>{showFlags[flag] ? "☑" : "☐"} {flag}</button>)}</div> : null}
                  </div>
                  <div className="engine-context-spacer" />
                  <button
                    type="button"
                    className={`engine-context-btn engine-inspector-toggle${narrowInspectorOpen ? " active" : ""}`}
                    aria-expanded={narrowInspectorOpen}
                    onClick={() => setNarrowInspectorOpen((open) => !open)}
                  >
                    Details
                  </button>
                  {isPlaying && playStats ? <span className="engine-context-stats">{Math.round(playStats.fps)} fps · {playStats.frameMs.toFixed(1)} ms</span> : null}
                  <div className="spawn-entity-wrap">
                    <button type="button" className={`engine-context-btn${viewportOptionsOpen ? " active" : ""}`} onClick={() => setViewportOptionsOpen((open) => !open)} aria-expanded={viewportOptionsOpen}>View options</button>
                    {viewportOptionsOpen ? (
                      <div className="engine-dropdown-menu right engine-view-options m-fade">
                        <label className="engine-menu-field"><span>Camera speed</span><input type="range" min={0.25} max={4} step={0.25} value={cameraSpeed} onChange={(event) => setCameraSpeed(Number(event.target.value))} /></label>
                        <label className="engine-menu-field"><span>Field of view · {viewportFov}°</span><input type="range" min={25} max={110} value={viewportFov} onChange={(event) => setViewportFov(Number(event.target.value))} /></label>
                        <label className="engine-menu-field"><span>Render scale</span><select className="engine-capability-select" value={screenPercentage} onChange={(event) => setScreenPercentage(Number(event.target.value))}><option value={50}>50%</option><option value={75}>75%</option><option value={100}>100%</option></select></label>
                        <button type="button" className="dropdown-item" onClick={() => { setViewportMaximized((value) => !value); setViewportOptionsOpen(false); }}>{viewportMaximized ? "Restore workspace" : "Maximise viewport"}</button>
                      </div>
                    ) : null}
                  </div>
                </div>
                <div className="engine-viewport-stage">
                  <EngineViewport
                    doc={viewDoc} touchedIds={isPlaying ? null : viewportTouched} selectedId={selectedId}
                    onSelect={(id) => handleSelect(id)} onTransform={(id, transform) => void handleTransform(id, transform)}
                    wireframe={wireframe} shadingMode={shadingMode} isPlaying={isPlaying} cameraMode={cameraMode}
                    gizmoMode={gizmoMode} gizmoSpace={gizmoSpace} snap={snap} empty={!isGame || viewDoc.entities.length === 0}
                    weather={(viewDoc.settings.weather as WeatherId) || "clear"} weatherPresets={presets} manifest={manifest}
                    showFlags={showFlags} cameraSpeed={cameraSpeed} fov={viewportFov} screenPercentage={screenPercentage}
                    hud={isPlaying ? playHud : null} playControls={isPlaying ? runtimeControls : null} active={active}
                    onDropAsset={(path) => void handleApplyAsset(path)}
                  />
                </div>
              </main>

              <EngineInspector
                entity={doc.entities.find((e) => e.id === selectedId) ?? null}
                entities={selection.map((id) => doc.entities.find((entity) => entity.id === id)).filter((entity): entity is SceneEntity => !!entity)}
                selectionCount={selection.length}
                onPatch={(id, component, value) => void handlePatchComponent(id, component, value)}
                onAddComponent={(id, component) => void handleAddComponent(id, component)}
                onRemoveComponent={(id, component) => void handleRemoveComponent(id, component)}
                onRename={(id, name) => void handleRename(id, name)}
                onSetTags={(id, tags) => void handleSetTags(id, tags)}
                narrowOpen={narrowInspectorOpen}
                onCloseNarrow={() => setNarrowInspectorOpen(false)}
              />
            </>
          )}
        </div>

        <EngineContentDrawer
          currentScenePath={activeScenePath}
          onSelectScene={(path) => void handleSelectScene(path)}
          onNewScene={() => setSceneNotice("New Scene lands with the scene-creation action in Phase 1 (ENG-110).")}
          isCollapsed={isDrawerCollapsed}
          onToggleCollapse={() => setIsDrawerCollapsed(!isDrawerCollapsed)}
          gameRoot={status?.game_root}
          isGame={isGame}
          onReplaceObject={(path) => void handleApplyAsset(path)}
          onImportReplace={() => void handleImportReplace()}
          onApplyAsset={(path) => void handleApplyAsset(path)}
          activeTab={drawerTab}
          height={drawerHeight}
          onHeightChange={setDrawerHeight}
          gameDebugRefreshToken={gameDebugRefreshToken}
          onActiveTabChange={(tab) => {
            setDrawerTab(tab);
            setIsDrawerCollapsed(false);
          }}
          outputLog={(
            <EngineOutputLog
              local={logLines}
              onClear={() => setLogLines([])}
              onReverted={() => void reload()}
            />
          )}
          problems={logLines.some((line) => line.level === "error") ? (
            <div className="engine-problems-list" role="list" aria-label="Engine problems">
              {logLines.filter((line) => line.level === "error").map((line) => (
                <div key={line.id} className="engine-problem-row" role="listitem">
                  <span>{line.channel}</span>
                  <strong>{line.text}</strong>
                  {line.source ? <button type="button" onClick={() => requestOpenWorkspaceFile(line.source!.path, line.source!.line)}>{line.source.path}:{line.source.line}</button> : null}
                </div>
              ))}
            </div>
          ) : (
            <div className="engine-drawer-empty"><strong>No active problems</strong><span>Validation and runtime errors will appear here without taking keyboard focus.</span></div>
          )}
        />
      </div>
    </div>
  );
}
function readDrawerPreference(projectPath: string): { collapsed: boolean; tab: EngineDrawerTab; height: number } {
  try {
    const parsed = JSON.parse(localStorage.getItem(`bhippi.engine.drawer.${projectPath}`) ?? "null") as { collapsed?: unknown; tab?: unknown; height?: unknown } | null;
    const tabs: EngineDrawerTab[] = ["content", "output", "problems", "activity", "game-debug", "builds"];
    return {
      collapsed: typeof parsed?.collapsed === "boolean" ? parsed.collapsed : true,
      tab: tabs.includes(parsed?.tab as EngineDrawerTab) ? parsed?.tab as EngineDrawerTab : "content",
      height: typeof parsed?.height === "number" && Number.isFinite(parsed.height)
        ? Math.max(160, Math.min(parsed.height, 620))
        : 205,
    };
  } catch {
    return { collapsed: true, tab: "content", height: 205 };
  }
}
