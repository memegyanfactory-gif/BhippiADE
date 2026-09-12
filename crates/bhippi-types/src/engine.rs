use crate::{BuildId, EntityId, SceneId};
use serde::{Deserialize, Serialize};
use specta::Type;

/// Maximum complete UTF-8 file carried by a chat workspace read/write tool.
pub const WORKSPACE_TOOL_MAX_FILE_BYTES: u64 = 65_536;

/// Hard ceiling on one autonomous engine loop (ADR-0060).
///
/// A bounded loop is a safety property, so there is always a last round. This is the
/// backstop, not the working limit: the loop normally ends because it stalled, because the
/// same rejected patch came back twice, or because the model asked the user. It is set high
/// enough that a turn which keeps making progress — reading a real project, then writing to
/// it — is never cut off partway.
pub const ENGINE_AUTONOMY_MAX_ROUNDS: usize = 24;

/// How many *consecutive* rounds may achieve nothing before the turn returns control
/// (ADR-0060).
///
/// This is the repair bound the loop has always needed: a model that cannot fix itself given
/// the real engine error and a fresh observation must hand back rather than thrash. A round
/// counts as achieving nothing only when no batch applied *and* every question it asked had
/// already been answered — reading the project for the first time is progress, and used to
/// spend this budget, which is how a turn could die having learned everything and written
/// nothing. Five keeps exactly the repair headroom the old flat six-round cap allowed.
pub const ENGINE_AUTONOMY_MAX_STALLED_ROUNDS: usize = 5;

/// Maximum size of the retrieval-shaped engine context injected before a turn. Deeper
/// scene facts stay behind `engine_query`, so a large level never crowds out the task.
pub const ENGINE_CONTEXT_TOKEN_BUDGET: u64 = 1_500;

/// Renderer captures are bounded before decoding so a compromised webview cannot make the
/// app allocate an arbitrary base64 payload.
pub const ENGINE_SCREENSHOT_MAX_BYTES: usize = 16 * 1024 * 1024;
pub const ENGINE_SCREENSHOT_MAX_DIMENSION: u32 = 8_192;

/// A capture/playtest request must fail loudly when no Engine pane answers it.
pub const ENGINE_OBSERVATION_TIMEOUT_SECS: u64 = 12;

/// Scripted playtests use fixed steps so an input sequence is replayable and bounded.
pub const ENGINE_PLAYTEST_MAX_STEPS: usize = 64;
pub const ENGINE_PLAYTEST_MAX_FRAMES_PER_STEP: u32 = 600;
pub const ENGINE_PLAYTEST_FIXED_DELTA_SECONDS: f32 = 1.0 / 60.0;
pub const ENGINE_PLAYTEST_MAX_KEYS_PER_STEP: usize = 16;
pub const ENGINE_PLAYTEST_MAX_KEY_CODE_BYTES: usize = 40;

/// Number of immutable `/gamedebug` JSON/Markdown report pairs retained per project.
/// The latest pointer is separate and is never counted as a run artefact.
pub const ENGINE_GAME_DEBUG_RETAINED_RUNS: usize = 20;

/// Facts about the game-engine workbench (ADR-0020). All variants are emitted through the
/// existing event bus and coalesced (INV-021 / INV-076): the 3D viewport itself never
/// redraws over IPC.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum EngineEvent {
    GameOpened {
        game: crate::GameId,
        name: String,
    },
    GameClosed {
        game: crate::GameId,
    },
    SceneOpened {
        scene: SceneId,
    },
    SceneDirty {
        dirty: bool,
    },
    SelectionChanged {
        entities: Vec<EntityId>,
    },
    HierarchyChanged {
        revision: u64,
    },
    TransformsUpdated {
        batch: Vec<EntityTransformPatch>,
    },
    AssetIndexChanged {
        revision: u64,
    },
    PlayStateChanged {
        state: PlayState,
    },
    PlayStats {
        fps: f32,
        frame_ms: f32,
        entities: u32,
        draw_calls: u32,
    },
    ConsoleLine {
        level: EngineLogLevel,
        target: String,
        text: String,
    },
    BuildProgress {
        build: BuildId,
        pct: u8,
        step: String,
    },
    BuildFinished {
        build: BuildId,
        ok: bool,
        artifact_path: Option<String>,
    },
    ViewportStatus {
        alive: bool,
        gpu_name: Option<String>,
    },
    EngineActionApplied {
        transaction: EngineTransactionSummary,
    },
    MindMapRegenerated {
        revision: u64,
    },
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum PlayState {
    Stop,
    Playing,
    Paused,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum EngineLogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

/// One entity whose transform moved (gizmo drag, agent edit, physics in play mode).
/// Coalesced upstream to ≤20/s (INV-076).
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct EntityTransformPatch {
    pub id: EntityId,
    pub pos: Option<[f32; 3]>,
    pub rot: Option<[f32; 4]>,
    pub scale: Option<[f32; 3]>,
}

/// The journal fact for an applied transaction (INV-071): what the human (or the agent)
/// changed, who did it, and what it is called — the ActivityDock step and the audit trail.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct EngineTransactionSummary {
    pub label: String,
    pub actor: EngineActor,
    pub op_count: usize,
    pub touched: Vec<EntityId>,
    pub scene: SceneId,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum EngineActor {
    User,
    Agent,
    System,
}

/// Image preview payload limit, independent of editable text size.
pub const WORKSPACE_IMAGE_MAX_BYTES: u64 = 67_108_864;
/// Asset import bound for the interactive model inspector.
pub const WORKSPACE_MODEL_MAX_BYTES: u64 = 536_870_912;
/// Bound on an asset viewer reaching a ready render state.
pub const ASSET_PREVIEW_START_TIMEOUT_SECS: u64 = 90;
