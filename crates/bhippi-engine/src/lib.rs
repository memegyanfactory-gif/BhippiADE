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
pub mod api;
pub mod asset;
pub mod assets;
pub mod capability;
pub mod document;
pub mod error;
pub mod game_debug;
pub mod game_inspector;
pub mod game_quality;
pub mod game_quality_baseline;
pub mod game_quality_corpus;
pub mod game_repair;
pub mod game_spec;
pub mod gates;
pub mod godot;
pub mod intent;
pub mod manifest;
pub mod orchestration;
pub mod procedural;
pub mod query;
pub mod registry;
pub mod scaffold;
pub mod transaction;

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
