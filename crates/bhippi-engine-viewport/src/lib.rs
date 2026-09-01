//! The engine editor control-channel protocol (ADR-0020, amended by **ADR-0028**).
//!
//! ADR-0020 specified the viewport as a Bevy child process driven over this channel.
//! ADR-0028 withdrew that: the webview viewport (`ui/src/engine/EngineViewport.tsx`) is the
//! shipping renderer, and Phase 6's runtime lives beside it.
//!
//! What remains here is [`protocol`] — a complete, well-formed description of an editor
//! control channel. It is **not currently used by anything**. It is kept because it is the
//! starting point if a native renderer is ever reinstated (ADR-0028 "Reversal"), and because
//! deleting a good design costs more than the file does.
//!
//! The 13-line Bevy stub that used to sit beside it was removed with the decision: a stub
//! that cannot open a window makes a crate look half-built rather than unbuilt.

#![cfg_attr(
    test,
    allow(clippy::expect_used, clippy::unwrap_used),
    doc = "Tests may panic on purpose: `expect` is how a test states its precondition, and a panic there is a failing test rather than a crashed app. The workspace `deny` stands everywhere else."
)]

pub mod protocol;

/// The version of the control-channel protocol, were it ever driven again.
pub const PROTOCOL_VERSION: &str = "editor.rpc.v1";
