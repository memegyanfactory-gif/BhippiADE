//! A fix an inspector proposes but never performs (ADR-0056 §5, INV-096).
//!
//! The separation the whole subsystem rests on is here. An inspector may *describe* a
//! repair in the engine's own typed vocabulary — [`GodotAction`], the same verbs the agent
//! uses, check-compiled and journalled by the same code — but the description is inert.
//! Turning it into bytes on disk needs a token, and the only thing that mints a token is
//! showing the preview to a person. There is no code path from "an inspector found
//! something" to "a file changed" that does not pass through a human looking at a diff.
//!
//! Nothing in this module writes. The proposal carries the actions and the files they would
//! touch; `bhippi-app` is what puts them through `lower` → `apply_changeset` → the journal,
//! and only after the token matches.

use crate::error::{EngineError, Result};
use crate::godot::action::GodotAction;
use bhippi_types::{FixRisk, INSPECT_MAX_FIX_ACTIONS};
use serde::{Deserialize, Serialize};
use specta::Type;
use std::collections::BTreeSet;

/// A repair, described. Applying it is somebody else's decision.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct ProposedFix {
    /// One line: what the change is, in the project's terms.
    pub summary: String,
    pub risk: FixRisk,
    /// The typed actions, in order. The agent never hand-writes a scene file (R8) and
    /// neither does a fix.
    pub actions: Vec<GodotAction>,
    /// One human line per action, for the preview card.
    pub steps: Vec<String>,
    /// Project-relative files the actions would touch, sorted and de-duplicated.
    pub files: Vec<String>,
    /// The token [`crate::inspect::fix::fix_token`] derives from the finding and these
    /// exact actions. An apply that does not carry it is refused.
    pub token: String,
}

impl ProposedFix {
    /// Describe a repair.
    ///
    /// # Errors
    /// When the summary is blank, when there are no actions, or when the batch is larger
    /// than [`INSPECT_MAX_FIX_ACTIONS`] — past that it is a task for an agent (§18), not a
    /// one-click repair a person can read in a card.
    pub fn new(
        finding_code: &str,
        summary: impl Into<String>,
        risk: FixRisk,
        actions: Vec<GodotAction>,
    ) -> Result<Self> {
        let summary = summary.into();
        if summary.trim().is_empty() {
            return Err(EngineError::Schema(
                format!("proposed fix for {finding_code} has no summary"),
                Some("A fix the user cannot read is a fix they cannot approve.".to_owned()),
            ));
        }
        if actions.is_empty() {
            return Err(EngineError::Schema(
                format!("proposed fix for {finding_code} has no actions"),
                Some(
                    "Describe the repair with engine actions, or leave the finding without \
                     a fix and let the recommendation stand."
                        .to_owned(),
                ),
            ));
        }
        if actions.len() > INSPECT_MAX_FIX_ACTIONS {
            return Err(EngineError::Schema(
                format!(
                    "proposed fix for {finding_code} has {} actions, over the {INSPECT_MAX_FIX_ACTIONS} a preview card can honestly show",
                    actions.len()
                ),
                Some("Send it to an agent as a task instead.".to_owned()),
            ));
        }

        let steps = actions.iter().map(GodotAction::to_label).collect();
        let files = touched_files(&actions);
        let token = fix_token(finding_code, &actions);
        Ok(Self {
            summary,
            risk,
            actions,
            steps,
            files,
            token,
        })
    }
}

/// Which files a batch of actions would touch, for the preview's "files affected" line.
///
/// Exhaustive on the action vocabulary so a new verb cannot silently drop out of the
/// preview and change a file the card did not name.
#[must_use]
pub fn touched_files(actions: &[GodotAction]) -> Vec<String> {
    let mut files: BTreeSet<String> = BTreeSet::new();
    for action in actions {
        match action {
            GodotAction::CreateScene { path, .. } | GodotAction::DeleteScene { path } => {
                files.insert(crate::godot::res_to_rel(path));
            }
            GodotAction::WriteScript { path, .. } | GodotAction::DeleteScript { path } => {
                files.insert(crate::godot::res_to_rel(path));
            }
            GodotAction::SetMainScene { .. }
            | GodotAction::SetProjectName { .. }
            | GodotAction::AddAutoload { .. }
            | GodotAction::AddInputAction { .. } => {
                files.insert("project.godot".to_owned());
            }
            GodotAction::AddNode { scene, .. }
            | GodotAction::RemoveNode { scene, .. }
            | GodotAction::RenameNode { scene, .. }
            | GodotAction::ReparentNode { scene, .. }
            | GodotAction::SetProperty { scene, .. }
            | GodotAction::RemoveProperty { scene, .. }
            | GodotAction::AddToGroup { scene, .. }
            | GodotAction::AttachScript { scene, .. }
            | GodotAction::InstanceScene { scene, .. }
            | GodotAction::AddSubResource { scene, .. }
            | GodotAction::ConnectSignal { scene, .. } => {
                files.insert(crate::godot::res_to_rel(scene));
            }
        }
    }
    files.into_iter().collect()
}

/// The approval token for one fix: the finding's check plus the exact actions.
///
/// Content-addressed on purpose. If the project moves under the user between the preview
/// and the click — a scene edited, the fix recomputed against new bytes — the recomputed
/// token differs and the stale approval is refused rather than applied to a project that is
/// no longer the one the person read about.
#[must_use]
pub fn fix_token(finding_code: &str, actions: &[GodotAction]) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(finding_code.as_bytes());
    for action in actions {
        hasher.update(b"\x1f");
        hasher.update(action.kind().as_bytes());
        // `to_label` is the human line; the bytes that matter are the serialised action.
        match serde_json::to_vec(action) {
            Ok(bytes) => hasher.update(&bytes),
            // An action that will not serialise cannot be applied either; hashing its
            // label keeps the token defined, and `lower` refuses the batch later.
            Err(_) => hasher.update(action.to_label().as_bytes()),
        };
    }
    format!("fix_{}", &hasher.finalize().to_hex()[..24])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::godot::tscn::TscnValue;

    fn connect() -> GodotAction {
        GodotAction::ConnectSignal {
            scene: "scenes/main.tscn".to_owned(),
            from: "Door/Area3D".to_owned(),
            signal: "body_entered".to_owned(),
            to: "Door".to_owned(),
            method: "_on_body_entered".to_owned(),
        }
    }

    #[test]
    fn a_fix_names_its_files_and_its_steps() {
        let fix = ProposedFix::new(
            "BHP-INS-302",
            "Connect the door's overlap to its interaction",
            FixRisk::Low,
            vec![connect()],
        )
        .expect("the fix is well formed");
        assert_eq!(fix.files, vec!["scenes/main.tscn".to_owned()]);
        assert_eq!(fix.steps.len(), 1);
        assert!(fix.steps[0].contains("body_entered"));
    }

    #[test]
    fn a_fix_with_no_summary_or_no_actions_is_refused() {
        assert!(ProposedFix::new("X", "  ", FixRisk::Low, vec![connect()]).is_err());
        assert!(ProposedFix::new("X", "do it", FixRisk::Low, Vec::new()).is_err());
    }

    #[test]
    fn a_batch_too_large_to_read_is_a_task_not_a_one_click_fix() {
        let actions: Vec<GodotAction> = (0..INSPECT_MAX_FIX_ACTIONS + 1)
            .map(|index| GodotAction::SetProperty {
                scene: "scenes/main.tscn".to_owned(),
                path: format!("Node{index}"),
                property: "visible".to_owned(),
                value: TscnValue::Bool(true),
            })
            .collect();
        assert!(ProposedFix::new("X", "many", FixRisk::Medium, actions).is_err());
    }

    #[test]
    fn the_token_changes_when_any_action_changes() {
        let first = fix_token("BHP-INS-302", &[connect()]);
        assert_eq!(first, fix_token("BHP-INS-302", &[connect()]));

        let GodotAction::ConnectSignal {
            scene,
            from,
            signal,
            to,
            ..
        } = connect()
        else {
            panic!("connect() builds a ConnectSignal");
        };
        let elsewhere = GodotAction::ConnectSignal {
            scene,
            from,
            signal,
            to,
            method: "_on_something_else".to_owned(),
        };
        assert_ne!(first, fix_token("BHP-INS-302", &[elsewhere]));
        assert_ne!(first, fix_token("BHP-INS-999", &[connect()]));
    }

    #[test]
    fn a_project_setting_action_names_project_godot_as_the_file_it_touches() {
        let files = touched_files(&[GodotAction::AddInputAction {
            name: "interact".to_owned(),
            events: Vec::new(),
            keycodes: vec![69],
            deadzone: None,
        }]);
        assert_eq!(files, vec!["project.godot".to_owned()]);
    }
}
