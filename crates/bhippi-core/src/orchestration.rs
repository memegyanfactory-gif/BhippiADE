//! Versioned, bounded context artifacts for capability-first engine work (ADR-0040).
//! These types deliberately reference engine-owned objects by format and hash so core does not
//! acquire an upward dependency on `bhippi-engine`.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const PROJECT_STATE_FORMAT: &str = "bhippi-project-state@1";
pub const TASK_CHECKPOINT_FORMAT: &str = "bhippi-task-checkpoint@1";
pub const AGENT_ARTIFACT_FORMAT: &str = "bhippi-agent-artifact@1";
pub const CONTEXT_BUDGET_FORMAT: &str = "bhippi-context-budget@1";

#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(deny_unknown_fields)]
pub struct GenericArtifactRef {
    pub format: String,
    pub hash: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct EvidenceRef {
    pub id: String,
    pub kind: String,
    pub content_hash: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ArtifactLimits {
    pub max_items_per_section: usize,
    pub max_text_bytes: usize,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectState {
    pub format: String,
    pub registry: GenericArtifactRef,
    pub game_spec: GenericArtifactRef,
    pub schema: GenericArtifactRef,
    pub active_level: Option<String>,
    pub actors: Vec<String>,
    pub mechanics: Vec<String>,
    pub bindings: Vec<String>,
    pub environment: Vec<String>,
    pub known_issues: Vec<String>,
}

impl ProjectState {
    pub fn validate(&self, limits: ArtifactLimits) -> Result<(), String> {
        require_format(&self.format, PROJECT_STATE_FORMAT)?;
        validate_ref(&self.registry)?;
        validate_ref(&self.game_spec)?;
        validate_ref(&self.schema)?;
        validate_sections(
            [
                self.actors.as_slice(),
                self.mechanics.as_slice(),
                self.bindings.as_slice(),
                self.environment.as_slice(),
                self.known_issues.as_slice(),
            ],
            limits,
        )
    }

    pub fn canonical_hash(&self) -> Result<String, String> {
        canonical_hash(self)
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TaskCheckpoint {
    pub format: String,
    pub project_state: GenericArtifactRef,
    pub goal: String,
    pub constraints: Vec<String>,
    pub unresolved_approvals: Vec<String>,
    pub decisions: Vec<String>,
    pub selected_capability_ids: Vec<String>,
    pub changes: Vec<String>,
    pub evidence: Vec<EvidenceRef>,
    pub files: Vec<String>,
    pub transaction_ids: Vec<String>,
    pub failures: Vec<String>,
    pub remaining_work: Vec<String>,
    pub next_action: String,
}

impl TaskCheckpoint {
    pub fn validate(&self, limits: ArtifactLimits) -> Result<(), String> {
        require_format(&self.format, TASK_CHECKPOINT_FORMAT)?;
        validate_ref(&self.project_state)?;
        require_text("goal", &self.goal, limits.max_text_bytes)?;
        require_text("next_action", &self.next_action, limits.max_text_bytes)?;
        validate_sections(
            [
                self.constraints.as_slice(),
                self.unresolved_approvals.as_slice(),
                self.decisions.as_slice(),
                self.selected_capability_ids.as_slice(),
                self.changes.as_slice(),
                self.files.as_slice(),
                self.transaction_ids.as_slice(),
                self.failures.as_slice(),
                self.remaining_work.as_slice(),
            ],
            limits,
        )?;
        if self.evidence.len() > limits.max_items_per_section {
            return Err("checkpoint evidence exceeds its item limit".to_owned());
        }
        for evidence in &self.evidence {
            require_text("evidence id", &evidence.id, limits.max_text_bytes)?;
            require_text("evidence kind", &evidence.kind, limits.max_text_bytes)?;
            require_hash(&evidence.content_hash)?;
        }
        Ok(())
    }

    pub fn canonical_hash(&self) -> Result<String, String> {
        canonical_hash(self)
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentRole {
    Coordinator,
    Worker,
    Tester,
    Reviewer,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AgentArtifact {
    pub format: String,
    pub from: AgentRole,
    pub to: AgentRole,
    pub registry: GenericArtifactRef,
    pub game_spec: GenericArtifactRef,
    pub project_state: GenericArtifactRef,
    pub checkpoint: GenericArtifactRef,
    pub evidence: Vec<EvidenceRef>,
}

impl AgentArtifact {
    pub fn validate(&self, limits: ArtifactLimits) -> Result<(), String> {
        require_format(&self.format, AGENT_ARTIFACT_FORMAT)?;
        for reference in [
            &self.registry,
            &self.game_spec,
            &self.project_state,
            &self.checkpoint,
        ] {
            validate_ref(reference)?;
        }
        if self.evidence.is_empty() {
            return Err("agent artifact requires evidence".to_owned());
        }
        if self.evidence.len() > limits.max_items_per_section {
            return Err("agent artifact evidence exceeds its item limit".to_owned());
        }
        for evidence in &self.evidence {
            require_hash(&evidence.content_hash)?;
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ContextPressure {
    Safety,
    Permissions,
    Blockers,
    RequiredEvidence,
    StableSystem,
    ProjectState,
    CapabilityContracts,
    GenericTools,
    Conversation,
    Handoff,
}

impl ContextPressure {
    const fn protected(self) -> bool {
        matches!(
            self,
            Self::Safety | Self::Permissions | Self::Blockers | Self::RequiredEvidence
        )
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BudgetRule {
    pub category: ContextPressure,
    pub soft_tokens: u64,
    pub hard_tokens: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ContextBudgetManifest {
    pub format: String,
    pub rules: Vec<BudgetRule>,
}

impl ContextBudgetManifest {
    pub fn validate(&self) -> Result<(), String> {
        require_format(&self.format, CONTEXT_BUDGET_FORMAT)?;
        let mut seen = BTreeSet::new();
        for rule in &self.rules {
            if rule.soft_tokens > rule.hard_tokens || rule.hard_tokens == 0 {
                return Err("context budget requires 0 <= soft <= hard and hard > 0".to_owned());
            }
            if !seen.insert(rule.category) {
                return Err("context budget contains a duplicate category".to_owned());
            }
        }
        Ok(())
    }

    pub fn canonical_hash(&self) -> Result<String, String> {
        self.validate()?;
        let mut canonical = self.clone();
        canonical.rules.sort_by_key(|rule| rule.category);
        canonical_hash(&canonical)
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case", tag = "decision")]
pub enum BudgetDecision {
    Within,
    Compact { categories: Vec<ContextPressure> },
    Retrieve { categories: Vec<ContextPressure> },
    Blocked { protected: Vec<ContextPressure> },
}

pub fn evaluate_budget(
    manifest: &ContextBudgetManifest,
    usage: &BTreeMap<ContextPressure, u64>,
) -> Result<BudgetDecision, String> {
    manifest.validate()?;
    let mut protected = Vec::new();
    let mut retrieve = Vec::new();
    let mut compact = Vec::new();
    for rule in &manifest.rules {
        let used = usage.get(&rule.category).copied().unwrap_or_default();
        if used > rule.hard_tokens {
            if rule.category.protected() {
                protected.push(rule.category);
            } else {
                retrieve.push(rule.category);
            }
        } else if used > rule.soft_tokens && !rule.category.protected() {
            compact.push(rule.category);
        }
    }
    if !protected.is_empty() {
        Ok(BudgetDecision::Blocked { protected })
    } else if !retrieve.is_empty() {
        Ok(BudgetDecision::Retrieve {
            categories: retrieve,
        })
    } else if !compact.is_empty() {
        Ok(BudgetDecision::Compact {
            categories: compact,
        })
    } else {
        Ok(BudgetDecision::Within)
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct StablePrefixManifest {
    pub system_hash: String,
    pub safety_hash: String,
    pub registry_hash: String,
    pub tool_schema_hash: String,
}

impl StablePrefixManifest {
    pub fn canonical_hash(&self) -> Result<String, String> {
        for hash in [
            &self.system_hash,
            &self.safety_hash,
            &self.registry_hash,
            &self.tool_schema_hash,
        ] {
            require_hash(hash)?;
        }
        canonical_hash(self)
    }
}

#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CapabilityCacheKey {
    pub capability_id: String,
    pub capability_version: String,
    pub registry_hash: String,
    pub engine_build_hash: String,
    pub material_task_fingerprint: String,
}

impl CapabilityCacheKey {
    pub fn changed_dimensions(&self, next: &Self) -> Vec<CacheInvalidation> {
        let mut reasons = Vec::new();
        if self.capability_id != next.capability_id
            || self.capability_version != next.capability_version
        {
            reasons.push(CacheInvalidation::Capability);
        }
        if self.registry_hash != next.registry_hash {
            reasons.push(CacheInvalidation::Registry);
        }
        if self.engine_build_hash != next.engine_build_hash {
            reasons.push(CacheInvalidation::EngineBuild);
        }
        if self.material_task_fingerprint != next.material_task_fingerprint {
            reasons.push(CacheInvalidation::TaskFingerprint);
        }
        reasons
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CacheInvalidation {
    Capability,
    Registry,
    EngineBuild,
    TaskFingerprint,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TokenQualityEvidence {
    pub measured_input_tokens: Option<u64>,
    pub estimated_input_tokens: u64,
    pub task_passed: bool,
    pub safety_failures: u32,
    pub repair_turns: u32,
    pub quality_score_milli: u32,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RegressionPolicy {
    pub maximum_quality_drop_milli: u32,
    pub maximum_extra_repairs: u32,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case", tag = "decision")]
pub enum TokenQualityDecision {
    NotMeasured,
    NoTokenImprovement,
    RejectedQualityRegression,
    Accepted { saved_tokens: u64 },
}

#[must_use]
pub fn evaluate_token_quality(
    baseline: &TokenQualityEvidence,
    candidate: &TokenQualityEvidence,
    policy: RegressionPolicy,
) -> TokenQualityDecision {
    let (Some(baseline_tokens), Some(candidate_tokens)) = (
        baseline.measured_input_tokens,
        candidate.measured_input_tokens,
    ) else {
        return TokenQualityDecision::NotMeasured;
    };
    if candidate_tokens >= baseline_tokens {
        return TokenQualityDecision::NoTokenImprovement;
    }
    let quality_floor = baseline
        .quality_score_milli
        .saturating_sub(policy.maximum_quality_drop_milli);
    if !candidate.task_passed
        || candidate.safety_failures > baseline.safety_failures
        || candidate.repair_turns
            > baseline
                .repair_turns
                .saturating_add(policy.maximum_extra_repairs)
        || candidate.quality_score_milli < quality_floor
    {
        return TokenQualityDecision::RejectedQualityRegression;
    }
    TokenQualityDecision::Accepted {
        saved_tokens: baseline_tokens - candidate_tokens,
    }
}

fn validate_ref(reference: &GenericArtifactRef) -> Result<(), String> {
    if reference
        .format
        .rsplit_once('@')
        .and_then(|(_, major)| major.parse::<u32>().ok())
        != Some(1)
    {
        return Err("artifact reference requires a supported @1 format".to_owned());
    }
    require_hash(&reference.hash)
}

fn validate_sections<'a>(
    sections: impl IntoIterator<Item = &'a [String]>,
    limits: ArtifactLimits,
) -> Result<(), String> {
    for section in sections {
        if section.len() > limits.max_items_per_section {
            return Err("artifact section exceeds its item limit".to_owned());
        }
        let mut seen = BTreeSet::new();
        for item in section {
            require_text("artifact item", item, limits.max_text_bytes)?;
            if !seen.insert(item) {
                return Err("artifact section contains a duplicate item".to_owned());
            }
        }
    }
    Ok(())
}

fn require_format(actual: &str, expected: &str) -> Result<(), String> {
    if actual == expected {
        Ok(())
    } else {
        Err(format!("unsupported format {actual}; expected {expected}"))
    }
}

fn require_text(label: &str, value: &str, max: usize) -> Result<(), String> {
    if value.trim().is_empty() {
        Err(format!("{label} must not be empty"))
    } else if value.len() > max {
        Err(format!("{label} exceeds its byte limit"))
    } else {
        Ok(())
    }
}

fn require_hash(value: &str) -> Result<(), String> {
    if value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        Ok(())
    } else {
        Err("artifact hash must be 64 hexadecimal characters".to_owned())
    }
}

fn canonical_hash<T: Serialize>(value: &T) -> Result<String, String> {
    serde_json::to_vec(value)
        .map(|bytes| blake3::hash(&bytes).to_hex().to_string())
        .map_err(|error| format!("artifact cannot be serialised: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hash(character: char) -> String {
        std::iter::repeat_n(character, 64).collect()
    }
    fn reference(format: &str, character: char) -> GenericArtifactRef {
        GenericArtifactRef {
            format: format.to_owned(),
            hash: hash(character),
        }
    }

    #[test]
    fn checkpoint_round_trip_is_stable_and_bounded() {
        let checkpoint = TaskCheckpoint {
            format: TASK_CHECKPOINT_FORMAT.to_owned(),
            project_state: reference(PROJECT_STATE_FORMAT, 'a'),
            goal: "Build jump".to_owned(),
            constraints: vec!["No source changes".to_owned()],
            unresolved_approvals: Vec::new(),
            decisions: vec!["Use physics.body".to_owned()],
            selected_capability_ids: vec!["physics.body".to_owned()],
            changes: vec!["Added body".to_owned()],
            evidence: vec![EvidenceRef {
                id: "test-1".to_owned(),
                kind: "test".to_owned(),
                content_hash: hash('b'),
            }],
            files: vec!["scene.json".to_owned()],
            transaction_ids: vec!["tx-1".to_owned()],
            failures: Vec::new(),
            remaining_work: vec!["Playtest".to_owned()],
            next_action: "Run playtest".to_owned(),
        };
        assert_eq!(
            checkpoint.validate(ArtifactLimits {
                max_items_per_section: 8,
                max_text_bytes: 128,
            }),
            Ok(())
        );
        let encoded = serde_json::to_string(&checkpoint)
            .unwrap_or_else(|error| panic!("checkpoint must encode: {error}"));
        let decoded: TaskCheckpoint = serde_json::from_str(&encoded)
            .unwrap_or_else(|error| panic!("checkpoint must decode: {error}"));
        assert_eq!(decoded, checkpoint);
        assert_eq!(decoded.canonical_hash(), checkpoint.canonical_hash());
    }

    #[test]
    fn protected_context_is_never_selected_for_compaction() {
        let manifest = ContextBudgetManifest {
            format: CONTEXT_BUDGET_FORMAT.to_owned(),
            rules: vec![
                BudgetRule {
                    category: ContextPressure::Safety,
                    soft_tokens: 10,
                    hard_tokens: 20,
                },
                BudgetRule {
                    category: ContextPressure::Conversation,
                    soft_tokens: 10,
                    hard_tokens: 20,
                },
            ],
        };
        let usage = BTreeMap::from([
            (ContextPressure::Safety, 21),
            (ContextPressure::Conversation, 15),
        ]);
        assert_eq!(
            evaluate_budget(&manifest, &usage),
            Ok(BudgetDecision::Blocked {
                protected: vec![ContextPressure::Safety]
            })
        );
    }

    #[test]
    fn every_cache_identity_dimension_invalidates() {
        let key = CapabilityCacheKey {
            capability_id: "a".to_owned(),
            capability_version: "1".to_owned(),
            registry_hash: hash('a'),
            engine_build_hash: hash('b'),
            material_task_fingerprint: hash('c'),
        };
        let mut next = key.clone();
        next.registry_hash = hash('d');
        assert_eq!(
            key.changed_dimensions(&next),
            vec![CacheInvalidation::Registry]
        );
        next = key.clone();
        next.engine_build_hash = hash('d');
        assert_eq!(
            key.changed_dimensions(&next),
            vec![CacheInvalidation::EngineBuild]
        );
        next = key.clone();
        next.material_task_fingerprint = hash('d');
        assert_eq!(
            key.changed_dimensions(&next),
            vec![CacheInvalidation::TaskFingerprint]
        );
    }

    #[test]
    fn token_savings_need_measured_non_regressing_quality() {
        let baseline = TokenQualityEvidence {
            measured_input_tokens: Some(1_000),
            estimated_input_tokens: 950,
            task_passed: true,
            safety_failures: 0,
            repair_turns: 1,
            quality_score_milli: 900,
        };
        let mut candidate = baseline.clone();
        candidate.measured_input_tokens = Some(700);
        candidate.quality_score_milli = 700;
        let policy = RegressionPolicy {
            maximum_quality_drop_milli: 50,
            maximum_extra_repairs: 0,
        };
        assert_eq!(
            evaluate_token_quality(&baseline, &candidate, policy),
            TokenQualityDecision::RejectedQualityRegression
        );
        candidate.quality_score_milli = 880;
        assert_eq!(
            evaluate_token_quality(&baseline, &candidate, policy),
            TokenQualityDecision::Accepted { saved_tokens: 300 }
        );
        candidate.measured_input_tokens = None;
        assert_eq!(
            evaluate_token_quality(&baseline, &candidate, policy),
            TokenQualityDecision::NotMeasured
        );
    }
}
