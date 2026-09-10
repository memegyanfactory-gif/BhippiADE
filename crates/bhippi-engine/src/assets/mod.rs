//! Assets subsystem for Godot game projects (ADR-0043, GAD Phase 6).
//!
//! Exposes procedural mesh, texture, and audio synthesis, the bundled CC0 library,
//! automatic licence attribution generation, and the three-tier resolution pipeline.

pub mod attribution;
pub mod cc0_library;
pub mod procedural_audio;
pub mod procedural_mesh;
pub mod procedural_texture;
pub mod resolver;

pub use attribution::{
    collect_project_attributions, generate_credits_markdown, write_project_credits,
    AssetAttribution, ProjectCredits,
};
pub use cc0_library::{materialize_cc0_asset, query_cc0_library, Cc0AssetEntry, CC0_CATALOGUE};
pub use procedural_audio::{
    generate_procedural_audio, write_procedural_audio, GeneratedAudio, ProceduralAudioPreset,
};
pub use procedural_mesh::{
    generate_procedural_mesh, write_procedural_mesh, GeneratedMesh, ProceduralMeshPreset,
};
pub use procedural_texture::{
    generate_procedural_texture, write_procedural_texture, GeneratedTexture,
    ProceduralTexturePattern,
};
pub use resolver::{resolve_asset, AssetRequest, ResolvedAsset};
