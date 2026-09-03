//! The intent compiler: a sentence in, a reviewable [`crate::game_spec::GameSpec`] out.
//!
//! The pipeline is Rust first, model second (`docs/16 §5.2`):
//!
//! 1. [`draft::draft`] reads the prompt deterministically — archetype, perspective,
//!    dimension, art words, settings, counts, win and lose phrases — for zero tokens.
//! 2. [`delta::spec_from_draft`] expands the matched [`archetype::Archetype`] into a
//!    complete, valid spec, folding in everything the draft settled.
//! 3. Only what is left goes to a model, and it may return only a
//!    [`delta::GameSpecDelta`], which [`delta::merge`] validates before it lands.
//! 4. [`questions::plan_readiness`] decides whether the plan may build: a Critical question
//!    blocks, a High one takes the archetype default and is flagged.
//! 5. Follow-ups go to [`fast_path::propose`] first, which turns "make the glide 20% longer"
//!    into one bounded parameter edit without a provider call at all.
//!
//! [`catalog`] holds the vocabulary all of this is allowed to name: the Godot 4 node classes
//! and the `preset.<domain>.<name>` cards. Nothing here performs I/O, spawns a process, or
//! touches the capability registry.

pub mod archetype;
pub mod catalog;
pub mod delta;
pub mod draft;
pub mod fast_path;
pub mod questions;

pub use archetype::{Archetype, ArchetypeQuestion, Dimension, MechanicTemplate, Perspective};
pub use delta::{delta_schema_excerpt, merge, spec_from_draft, GameSpecDelta, MAX_DELTA_ITEMS};
pub use draft::{draft, IntentDraft, IntentSlot};
pub use fast_path::{propose, FastPathContext, FastPathProposal};
pub use questions::{answer, plan_readiness, Readiness};
