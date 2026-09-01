//! Agent capabilities (ENG-190, plan §88).
//!
//! What the agent may do to *this* project, decided per project and stored in
//! `Bhippi.game.toml` so it travels with the game, is hand-editable, and is reviewable in a
//! diff. A capability switch is not a prompt instruction: the prompt is a courtesy, the
//! check is code, and a denied action is refused by the same path that refuses an invalid
//! component payload.
//!
//! Three states rather than two. "Ask" is the interesting one: the useful default is not
//! *may the agent delete things* but *may it delete things without showing me first*, and
//! collapsing that into allow/deny is what makes people turn the whole gate off.

use serde::{Deserialize, Serialize};
use specta::Type;
use std::collections::BTreeMap;
use std::fmt;

/// One thing an agent can do to a project.
///
/// The list is deliberately short. A capability is worth having only when a user would
/// plausibly answer differently for it than for its neighbours — otherwise it is a switch
/// nobody understands and everybody leaves alone.
#[derive(
    Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Type,
)]
#[serde(rename_all = "snake_case")]
pub enum Capability {
    /// Move, rename, re-parent, retag, or change components on entities that already exist.
    EditScene,
    /// Add entities, materials, shaders, prefabs — anything additive.
    CreateContent,
    /// Remove entities or components. The one genuinely lossy verb.
    Delete,
    /// Pull a file in from outside the project folder.
    Import,
    /// Write or overwrite a gameplay script (ADR-0030).
    WriteScript,
    /// Start play, drive input, take a screenshot.
    RunPlay,
    /// Produce a build artefact.
    Build,
}

impl Capability {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::EditScene => "edit_scene",
            Self::CreateContent => "create_content",
            Self::Delete => "delete",
            Self::Import => "import",
            Self::WriteScript => "write_script",
            Self::RunPlay => "run_play",
            Self::Build => "build",
        }
    }

    /// Every capability, in the order the settings panel lists them.
    pub const ALL: [Self; 7] = [
        Self::EditScene,
        Self::CreateContent,
        Self::Delete,
        Self::Import,
        Self::WriteScript,
        Self::RunPlay,
        Self::Build,
    ];

    /// A sentence explaining what turning this off actually stops.
    #[must_use]
    pub fn doc(self) -> &'static str {
        match self {
            Self::EditScene => "Move, rename, re-parent and re-component existing entities.",
            Self::CreateContent => "Add entities, materials, shaders and prefabs.",
            Self::Delete => "Remove entities and components.",
            Self::Import => "Bring a file in from outside the project folder.",
            Self::WriteScript => "Write or overwrite a gameplay script.",
            Self::RunPlay => "Start play, drive input and take screenshots.",
            Self::Build => "Produce a build artefact.",
        }
    }

    /// The shipped default for this capability.
    ///
    /// Edit and create are allowed because every write is transacted, journaled and on the
    /// same undo stack the user's own edits land on — stopping for each one teaches people
    /// to click through the dialog. Delete, import and build are asked for because they are
    /// respectively lossy, outside the project, and slow.
    #[must_use]
    pub fn default_decision(self) -> Decision {
        match self {
            Self::EditScene | Self::CreateContent | Self::WriteScript | Self::RunPlay => {
                Decision::Allow
            }
            Self::Delete | Self::Import | Self::Build => Decision::Ask,
        }
    }

    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|capability| capability.as_str() == name)
    }
}

impl fmt::Display for Capability {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// What the project says about one capability.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum Decision {
    /// Do it, and report it afterwards.
    Allow,
    /// Show the plan and wait for a yes.
    #[default]
    Ask,
    /// Refuse, with a sentence saying which switch to flip.
    Deny,
}

impl Decision {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Allow => "allow",
            Self::Ask => "ask",
            Self::Deny => "deny",
        }
    }

    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "allow" => Some(Self::Allow),
            "ask" => Some(Self::Ask),
            "deny" => Some(Self::Deny),
            _ => None,
        }
    }
}

/// The per-project policy, as it appears in `[agent]` in `Bhippi.game.toml`.
///
/// Absent keys take their default, so a manifest written before this existed keeps working
/// and a user only has to name the switches they actually changed.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize, Type)]
#[serde(default)]
pub struct CapabilityPolicy {
    #[serde(flatten)]
    overrides: BTreeMap<String, Decision>,
}

impl CapabilityPolicy {
    /// The decision in force for one capability.
    #[must_use]
    pub fn decision(&self, capability: Capability) -> Decision {
        self.overrides
            .get(capability.as_str())
            .copied()
            .unwrap_or_else(|| capability.default_decision())
    }

    pub fn set(&mut self, capability: Capability, decision: Decision) {
        if decision == capability.default_decision() {
            // Keep the manifest to what the user actually changed — a file full of restated
            // defaults is a file nobody reads.
            self.overrides.remove(capability.as_str());
        } else {
            self.overrides
                .insert(capability.as_str().to_owned(), decision);
        }
    }

    /// Every capability with its effective decision, for the settings panel and the prompt.
    #[must_use]
    pub fn effective(&self) -> Vec<(Capability, Decision)> {
        Capability::ALL
            .into_iter()
            .map(|capability| (capability, self.decision(capability)))
            .collect()
    }

    /// Reject anything in the file that is not a real capability or a real decision. A typo
    /// that silently means "default" is how a security switch becomes decorative.
    ///
    /// # Errors
    /// Names the offending key or value and lists the legal ones.
    pub fn validate(&self) -> crate::Result<()> {
        for key in self.overrides.keys() {
            if Capability::from_name(key).is_none() {
                return Err(crate::EngineError::Manifest(
                    format!("`[agent] {key}` is not a capability."),
                    Some(format!(
                        "Capabilities are: {}.",
                        Capability::ALL
                            .map(|capability| capability.as_str())
                            .join(", ")
                    )),
                ));
            }
        }
        Ok(())
    }
}

/// The capability an action kind needs.
///
/// Every kind the batch vocabulary accepts is named here. An unknown kind maps to
/// `EditScene`, which is the least-privileged bucket — but the action itself is rejected by
/// the batch parser long before this matters, so this is a floor, not a hole.
#[must_use]
pub fn capability_for(kind: &str) -> Capability {
    match kind {
        "delete" | "remove_component" => Capability::Delete,
        "spawn" | "duplicate" | "group_entities" | "scatter_entities" | "place_grid"
        | "place_ring" | "place_perimeter" | "place_stack" | "room_from_bounds"
        | "corridor_between" | "create_material" | "create_shader" | "create_prefab" => {
            Capability::CreateContent
        }
        "create_script" => Capability::WriteScript,
        "import_asset" | "import_folder" | "set_asset_license" => Capability::Import,
        "play" | "pause" | "step" | "stop" | "possess" | "screenshot" | "playtest" => {
            Capability::RunPlay
        }
        "build" | "package" => Capability::Build,
        _ => Capability::EditScene,
    }
}

/// What a policy says about a whole batch.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize, Type)]
pub struct CapabilityVerdict {
    /// Capabilities the batch needs that are denied outright, with the action kinds that
    /// asked for them. Non-empty means the batch does not run at all.
    pub denied: Vec<DeniedCapability>,
    /// Whether at least one action needs an explicit yes first.
    pub needs_approval: bool,
    /// Every capability the batch touches, for the plan card.
    pub required: Vec<Capability>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct DeniedCapability {
    pub capability: Capability,
    pub kinds: Vec<String>,
}

impl CapabilityVerdict {
    /// The refusal message, naming the switch to flip. A "permission denied" with no route
    /// to permission is a dead end, not a gate.
    #[must_use]
    pub fn refusal(&self) -> Option<String> {
        if self.denied.is_empty() {
            return None;
        }
        let detail = self
            .denied
            .iter()
            .map(|entry| format!("{} ({})", entry.capability, entry.kinds.join(", ")))
            .collect::<Vec<_>>()
            .join("; ");
        Some(format!(
            "This project does not allow the agent to: {detail}. Change it in \
             `Bhippi.game.toml` under `[agent]`, or in Engine → Agent permissions."
        ))
    }
}

/// Evaluate a batch's action kinds against a policy.
#[must_use]
pub fn evaluate(policy: &CapabilityPolicy, kinds: &[String]) -> CapabilityVerdict {
    let mut denied: BTreeMap<Capability, Vec<String>> = BTreeMap::new();
    let mut required: Vec<Capability> = Vec::new();
    let mut needs_approval = false;

    for kind in kinds {
        let capability = capability_for(kind);
        if !required.contains(&capability) {
            required.push(capability);
        }
        match policy.decision(capability) {
            Decision::Allow => {}
            Decision::Ask => needs_approval = true,
            Decision::Deny => {
                let entry = denied.entry(capability).or_default();
                if !entry.contains(kind) {
                    entry.push(kind.clone());
                }
            }
        }
    }
    required.sort_unstable();

    CapabilityVerdict {
        denied: denied
            .into_iter()
            .map(|(capability, kinds)| DeniedCapability { capability, kinds })
            .collect(),
        needs_approval,
        required,
    }
}

#[cfg(test)]
mod tests {
    use super::{capability_for, evaluate, Capability, CapabilityPolicy, Decision};

    fn kinds(list: &[&str]) -> Vec<String> {
        list.iter().map(|kind| (*kind).to_owned()).collect()
    }

    #[test]
    fn the_defaults_are_edit_and_create_allowed_delete_and_build_asked() {
        let policy = CapabilityPolicy::default();
        assert_eq!(policy.decision(Capability::EditScene), Decision::Allow);
        assert_eq!(policy.decision(Capability::CreateContent), Decision::Allow);
        assert_eq!(policy.decision(Capability::WriteScript), Decision::Allow);
        assert_eq!(policy.decision(Capability::RunPlay), Decision::Allow);
        assert_eq!(policy.decision(Capability::Delete), Decision::Ask);
        assert_eq!(policy.decision(Capability::Import), Decision::Ask);
        assert_eq!(policy.decision(Capability::Build), Decision::Ask);
    }

    #[test]
    fn an_additive_batch_runs_without_asking_under_the_defaults() {
        let verdict = evaluate(
            &CapabilityPolicy::default(),
            &kinds(&["spawn", "set_transform", "create_material"]),
        );
        assert!(verdict.denied.is_empty());
        assert!(!verdict.needs_approval);
        assert!(verdict.required.contains(&Capability::CreateContent));
        assert!(verdict.required.contains(&Capability::EditScene));
    }

    #[test]
    fn a_delete_asks_under_the_defaults_and_says_so_in_the_plan() {
        let verdict = evaluate(&CapabilityPolicy::default(), &kinds(&["spawn", "delete"]));
        assert!(verdict.needs_approval);
        assert!(verdict.denied.is_empty(), "ask is not deny");
        assert!(verdict.required.contains(&Capability::Delete));
    }

    #[test]
    fn a_denied_capability_refuses_the_whole_batch_and_names_the_switch() {
        let mut policy = CapabilityPolicy::default();
        policy.set(Capability::Delete, Decision::Deny);
        let verdict = evaluate(&policy, &kinds(&["spawn", "delete", "remove_component"]));

        let refusal = verdict.refusal().expect("a denial must explain itself");
        assert!(refusal.contains("delete"));
        assert!(refusal.contains("Bhippi.game.toml"));
        assert_eq!(verdict.denied.len(), 1);
        assert_eq!(verdict.denied[0].kinds, vec!["delete", "remove_component"]);
    }

    #[test]
    fn a_policy_only_records_what_differs_from_the_default() {
        let mut policy = CapabilityPolicy::default();
        policy.set(Capability::Delete, Decision::Ask); // already the default
        let toml = toml::to_string(&policy).expect("serialises");
        assert!(
            toml.trim().is_empty(),
            "restating a default must not write a key, got {toml:?}"
        );

        policy.set(Capability::Delete, Decision::Deny);
        let toml = toml::to_string(&policy).expect("serialises");
        assert!(toml.contains("delete"), "{toml}");
        assert!(toml.contains("deny"), "{toml}");
    }

    #[test]
    fn a_policy_round_trips_through_toml() {
        let mut policy = CapabilityPolicy::default();
        policy.set(Capability::Import, Decision::Deny);
        policy.set(Capability::Build, Decision::Allow);
        let text = toml::to_string(&policy).expect("serialises");
        let parsed: CapabilityPolicy = toml::from_str(&text).expect("parses");
        assert_eq!(parsed, policy);
        assert_eq!(parsed.decision(Capability::Import), Decision::Deny);
        assert_eq!(parsed.decision(Capability::Build), Decision::Allow);
        // Untouched capabilities still take their defaults.
        assert_eq!(parsed.decision(Capability::Delete), Decision::Ask);
    }

    #[test]
    fn a_misspelled_capability_is_refused_rather_than_ignored() {
        let policy: CapabilityPolicy = toml::from_str("delet = \"deny\"").expect("parses");
        let error = policy
            .validate()
            .expect_err("a typo must not silently mean the default");
        assert!(error.to_string().contains("delet"));
        assert!(error.hint().unwrap_or_default().contains("delete"));
    }

    #[test]
    fn every_action_kind_the_vocabulary_accepts_maps_to_a_capability() {
        // The mapping that matters: the lossy and outside-the-project verbs must not fall
        // into the additive bucket by omission.
        assert_eq!(capability_for("delete"), Capability::Delete);
        assert_eq!(capability_for("remove_component"), Capability::Delete);
        assert_eq!(capability_for("create_script"), Capability::WriteScript);
        assert_eq!(capability_for("import_asset"), Capability::Import);
        assert_eq!(capability_for("set_asset_license"), Capability::Import);
        assert_eq!(capability_for("spawn"), Capability::CreateContent);
        assert_eq!(capability_for("set_transform"), Capability::EditScene);
        // An unknown kind lands in the least-privileged bucket, never in Delete or Import.
        assert_eq!(capability_for("wat"), Capability::EditScene);
    }

    #[test]
    fn every_capability_is_documented_and_named() {
        for capability in Capability::ALL {
            assert!(!capability.doc().is_empty(), "{capability} has no doc");
            assert_eq!(Capability::from_name(capability.as_str()), Some(capability));
        }
        assert_eq!(Capability::from_name("root"), None);
        for decision in [Decision::Allow, Decision::Ask, Decision::Deny] {
            assert_eq!(Decision::from_name(decision.as_str()), Some(decision));
        }
    }
}
