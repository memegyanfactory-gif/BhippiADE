//! The game-engine editor domain (ADR-0020). Pure library: no windowing, no rendering,
//! no database. Scene state, transactions, undo, asset indexing, schema, mind-map
//! generation and the AI action channel all live here so the webview computes nothing
//! (INV-073) and the crate's tests run headless with zero GPU and zero DB.

#![cfg_attr(
    test,
    allow(clippy::expect_used, clippy::unwrap_used),
    doc = "Tests may panic on purpose: `expect` is how a test states its precondition, and a panic there is a failing test rather than a crashed app. The workspace `deny` stands everywhere else."
)]

pub mod action;
pub mod animation_contract;
pub mod api;
pub mod asset;
pub mod behavior_graph;
pub mod capability;
pub mod compose;
pub mod control_contract;
pub mod document;
pub mod error;
pub mod extension_contract;
pub mod game_debug;
pub mod game_inspector;
pub mod game_quality;
pub mod game_quality_corpus;
pub mod game_repair;
pub mod game_spec;
pub mod game_test_plan;
pub mod gameplay_contract;
pub mod gates;
pub mod hud;
pub mod hud_action;
pub mod input;
pub mod manifest;
pub mod material;
pub mod media_contract;
pub mod mesh;
pub mod mindmap;
pub mod navigation_ai;
pub mod network_contract;
pub mod orchestration;
pub mod physics_contract;
pub mod prefab;
pub mod procedural;
pub mod production_evidence;
pub mod profiler_contract;
pub mod query;
pub mod registry;
pub mod runtime_contract;
pub mod runtime_protocol;
pub mod runtime_save;
pub mod runtime_save_store;
pub mod scaffold;
pub mod schema;
pub mod script;
pub mod transaction;
pub mod weather;
pub mod world_contract;

pub use api::{
    AnimationGraphView, AssetDependenciesView, AssetDependency, AssetUser, AssetUsersView,
    ChildrenView, ComponentsView, EntityQuery, EntityRef, EntityView, Expansion, MaterialGraphView,
    ParentView, PhysicsView, SceneQueries, SceneView, ScriptsView, ShaderView,
};

pub use action::{BatchError, EngineAction, EngineActionBatch, EngineWallOpening};
pub use asset::{AssetIndex, AssetKind, AssetRecord, LicenseState};
pub use bhippi_types::EngineActor;
pub use document::SceneDocument;
pub use error::{EngineError, EngineErrorCode, Result};
pub use manifest::{EngineTrack, GameManifest};
pub use transaction::{EngineTransaction, EntitySpec, Op, Session, UndoStack};

/// Project-root marker: the file whose presence makes a Bhippi project a *game project*.
pub const GAME_MANIFEST_FILE: &str = "Bhippi.game.toml";

/// The editor memory budget for per-project undo stacks (plan §12).
pub const UNDO_STACK_CAP: usize = 500;
