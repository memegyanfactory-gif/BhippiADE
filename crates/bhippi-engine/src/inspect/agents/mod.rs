//! The nine specialists (ADR-0056 §2).
//!
//! Each one is a pure function from [`InspectContext`] to findings plus a statement of what
//! it actually saw. None of them reads the disk, none of them calls a model, and none of
//! them can write: everything they know came from the one snapshot the scan took, which is
//! what makes a scan reproducible and free.
//!
//! The rule every check in here follows: **report the fact, not the inference.** "This
//! `Area3D` has no `[connection]` block and no `connect(` call in its script" is a fact the
//! file supports; "this door is broken" is a conclusion the fact permits. The finding's
//! title may say the second only because its evidence says the first.

pub mod ai;
pub mod animation;
pub mod assets;
pub mod code;
pub mod gameplay;
pub mod performance;
pub mod physics;
pub mod scene;
pub mod ui;

use super::finding::{Finding, FindingDraft};

/// Build a draft into the list, or log the inspector's own bug and drop it.
///
/// A draft that fails to build is never the user's problem — it means an inspector wrote a
/// finding without one of the six answers. Dropping it keeps the scan useful; the log and
/// the per-inspector fixture tests are what make sure it does not stay dropped.
pub(crate) fn collect(out: &mut Vec<Finding>, draft: FindingDraft) {
    match draft.build() {
        Ok(finding) => out.push(finding),
        Err(error) => tracing::error!(%error, "an inspector produced an unreportable finding"),
    }
}
