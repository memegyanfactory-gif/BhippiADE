//! The bounded model pass, expressed as a contract rather than a provider call.
//!
//! Nothing here talks to a model. This module owns the two halves that make the pass cheap:
//! [`spec_from_draft`] expands an archetype into a complete, valid [`GameSpec`] for free, and
//! [`GameSpecDelta`] is the small, closed shape the model is allowed to return on top of it.
//! [`merge`] applies a delta and re-validates, so a model that invents a question id or
//! floods the turn is refused with a hint instead of corrupting the plan.

use crate::error::{EngineError, Result};
use crate::game_spec::{
    FactCertainty, GameConstraints, GameRequirement, GameSpec, MechanicContract, OpenQuestion,
    PlanningBudgets, SpecFact, GAME_SPEC_FORMAT,
};
use crate::intent::archetype::{
    Archetype, SpecBucket, REQ_CAMERA, REQ_HUD, REQ_LEVEL, REQ_PLAYER, REQ_RULES,
};
use crate::intent::catalog;
use crate::intent::draft::{
    IntentDraft, IntentSlot, ASSUMED_CONFIDENCE_BPS, CERTAIN_CONFIDENCE_BPS,
};
use serde::{Deserialize, Serialize};
use specta::Type;

/// The only delta format this build accepts.
pub const GAME_SPEC_DELTA_FORMAT: &str = "bhippi-game-spec-delta@1";

/// The largest number of changes one model turn may make. A delta is a correction to an
/// archetype expansion, not a second way to author a spec: past this the turn is refused and
/// the user is asked to narrow the change.
pub const MAX_DELTA_ITEMS: usize = 24;

/// One answered question inside a delta.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct QuestionAnswer {
    pub question_id: String,
    pub answer: String,
}

/// The small shape the model returns. Every list is additive except `remove_ids`, and every
/// field is optional, so an empty delta is legal and means "the archetype expansion is right".
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct GameSpecDelta {
    pub format: String,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub genre: Option<SpecFact>,
    #[serde(default)]
    pub player_loop_add: Vec<SpecFact>,
    #[serde(default)]
    pub mechanics_add: Vec<GameRequirement>,
    #[serde(default)]
    pub world_add: Vec<GameRequirement>,
    #[serde(default)]
    pub actors_add: Vec<GameRequirement>,
    #[serde(default)]
    pub ui_add: Vec<GameRequirement>,
    #[serde(default)]
    pub answers: Vec<QuestionAnswer>,
    #[serde(default)]
    pub remove_ids: Vec<String>,
}

impl GameSpecDelta {
    pub fn parse(text: &str) -> Result<Self> {
        let delta: Self = serde_json::from_str(text).map_err(|error| {
            schema(
                format!("invalid game spec delta: {error}"),
                format!(
                    "Return only the {GAME_SPEC_DELTA_FORMAT} shape; see delta_schema_excerpt()."
                ),
            )
        })?;
        Ok(delta)
    }

    /// How many changes this delta makes, counted the way [`MAX_DELTA_ITEMS`] bounds them.
    #[must_use]
    pub fn item_count(&self) -> usize {
        usize::from(self.title.is_some())
            + usize::from(self.genre.is_some())
            + self.player_loop_add.len()
            + self.mechanics_add.len()
            + self.world_add.len()
            + self.actors_add.len()
            + self.ui_add.len()
            + self.answers.len()
            + self.remove_ids.len()
    }
}

/// Expand an archetype into a complete, valid `GameSpec`, folding in everything the draft
/// already settled. No model call, no registry query, no I/O.
///
/// Questions the draft answered outright stay in `open_questions` with `resolved` set: the
/// plan card needs the decision on the record, and `compose_plan` reads resolution from
/// exactly this field.
pub fn spec_from_draft(drafted: &IntentDraft, pack: &Archetype) -> GameSpec {
    let certain = drafted.certain_values();
    let mut mechanics = Vec::new();
    let mut world = Vec::new();
    let mut actors = Vec::new();
    let mut ui = Vec::new();

    for row in pack.requirements() {
        let mut statement = row.statement.clone();
        for note in count_notes(drafted, &row.capability_id) {
            statement.push_str(&format!(" · {note}"));
        }
        let requirement = GameRequirement {
            confidence_bps: requirement_confidence(drafted, &row.requirement_id),
            id: row.requirement_id.clone(),
            statement,
            depends_on: depends_on(&row.requirement_id, row.bucket),
            preferred_capabilities: Vec::new(),
            maturity: crate::registry::MaturityRequirement::default(),
        };
        match row.bucket {
            SpecBucket::Mechanics => mechanics.push(requirement),
            SpecBucket::World => world.push(requirement),
            SpecBucket::Actors => actors.push(requirement),
            SpecBucket::Ui => ui.push(requirement),
        }
    }

    let open_questions = pack
        .questions
        .iter()
        .map(|question| {
            let mut options = question.options.clone();
            if let Some(default) = &question.default {
                options.retain(|option| option != default);
                options.insert(0, default.clone());
            }
            let resolved = options
                .iter()
                .find(|option| certain.contains(*option))
                .cloned();
            OpenQuestion {
                id: question.id.clone(),
                question: question.question.clone(),
                impact: question.impact,
                affects_requirements: question.affects.clone(),
                options,
                resolved,
            }
        })
        .collect();

    GameSpec {
        format: GAME_SPEC_FORMAT.to_owned(),
        title: title_for(drafted, pack),
        genre: drafted
            .fact(IntentSlot::Genre)
            .cloned()
            .unwrap_or(SpecFact {
                value: pack.id.clone(),
                confidence_bps: ASSUMED_CONFIDENCE_BPS,
                certainty: FactCertainty::Assumed,
                alternatives: Vec::new(),
            }),
        player_loop: pack
            .core_loop
            .iter()
            .map(|step| SpecFact {
                value: step.clone(),
                confidence_bps: ASSUMED_CONFIDENCE_BPS,
                certainty: FactCertainty::Assumed,
                alternatives: Vec::new(),
            })
            .collect(),
        mechanics,
        world,
        actors,
        ui,
        constraints: GameConstraints {
            platforms: pack.defaults.platforms.clone(),
            quality: drafted
                .facts_for(IntentSlot::ArtStyle)
                .into_iter()
                .cloned()
                .collect(),
            budgets: PlanningBudgets {
                turn_tokens: pack.defaults.turn_tokens,
                frame_time_micros: None,
                max_memory_mb: None,
                max_content_mb: None,
                max_capability_cost: None,
                max_new_extensions: pack.defaults.max_new_extensions,
            },
        },
        acceptance_mechanics: pack
            .acceptance
            .iter()
            .map(|mechanic| MechanicContract {
                id: mechanic.id.clone(),
                promise: mechanic.promise.clone(),
                setup: mechanic.setup.clone(),
                requirement_ids: mechanic.requires.clone(),
                deterministic_probes: mechanic.probes.clone(),
                expected_evidence: mechanic.evidence.clone(),
            })
            .collect(),
        open_questions,
    }
}

/// Apply a model delta to a spec and re-validate. The spec is only replaced if the merged
/// result is valid, so a refused delta leaves the plan exactly as the user last saw it.
pub fn merge(spec: &GameSpec, delta: &GameSpecDelta) -> Result<GameSpec> {
    if delta.format != GAME_SPEC_DELTA_FORMAT {
        return Err(schema(
            format!("unsupported game spec delta format {:?}", delta.format),
            format!("Return {GAME_SPEC_DELTA_FORMAT}; unknown major versions block."),
        ));
    }
    let items = delta.item_count();
    if items > MAX_DELTA_ITEMS {
        return Err(schema(
            format!("game spec delta carries {items} changes, the limit is {MAX_DELTA_ITEMS}"),
            "Split the change into smaller approved slices, or edit the plan card directly."
                .to_owned(),
        ));
    }

    let mut merged = spec.clone();
    if let Some(title) = &delta.title {
        merged.title = title.clone();
    }
    if let Some(genre) = &delta.genre {
        merged.genre = genre.clone();
    }
    merged.player_loop.extend(delta.player_loop_add.clone());

    for (additions, target) in [
        (&delta.mechanics_add, &mut merged.mechanics),
        (&delta.world_add, &mut merged.world),
        (&delta.actors_add, &mut merged.actors),
        (&delta.ui_add, &mut merged.ui),
    ] {
        for requirement in additions {
            match target.iter_mut().find(|held| held.id == requirement.id) {
                Some(held) => held.clone_from(requirement),
                None => target.push(requirement.clone()),
            }
        }
    }

    for id in &delta.remove_ids {
        let known = [
            &mut merged.mechanics,
            &mut merged.world,
            &mut merged.actors,
            &mut merged.ui,
        ]
        .into_iter()
        .any(|list| list.iter().any(|requirement| &requirement.id == id));
        if !known {
            return Err(schema(
                format!("game spec delta removes unknown requirement {id:?}"),
                "Remove only ids the current plan lists.".to_owned(),
            ));
        }
        if let Some(contract) = merged
            .acceptance_mechanics
            .iter()
            .find(|contract| contract.requirement_ids.contains(id))
        {
            return Err(schema(
                format!(
                    "requirement {id:?} still backs acceptance mechanic {:?}",
                    contract.id
                ),
                "Remove or rewrite the acceptance mechanic before dropping what it tests."
                    .to_owned(),
            ));
        }
        for list in [
            &mut merged.mechanics,
            &mut merged.world,
            &mut merged.actors,
            &mut merged.ui,
        ] {
            list.retain(|requirement| &requirement.id != id);
        }
        for list in [
            &mut merged.mechanics,
            &mut merged.world,
            &mut merged.actors,
            &mut merged.ui,
        ] {
            for requirement in list.iter_mut() {
                requirement.depends_on.retain(|dependency| dependency != id);
            }
        }
        for question in &mut merged.open_questions {
            question
                .affects_requirements
                .retain(|affected| affected != id);
        }
    }

    for answer in &delta.answers {
        let question = merged
            .open_questions
            .iter_mut()
            .find(|question| question.id == answer.question_id)
            .ok_or_else(|| {
                schema(
                    format!(
                        "game spec delta answers unknown question {:?}",
                        answer.question_id
                    ),
                    "Answer only the question ids the plan card lists.".to_owned(),
                )
            })?;
        if !question.options.is_empty() && !question.options.contains(&answer.answer) {
            return Err(schema(
                format!(
                    "question {:?} has no option {:?}",
                    answer.question_id, answer.answer
                ),
                format!("Choose one of: {}.", question.options.join(", ")),
            ));
        }
        question.resolved = Some(answer.answer.clone());
    }

    for list in [
        &mut merged.mechanics,
        &mut merged.world,
        &mut merged.actors,
        &mut merged.ui,
    ] {
        list.sort_by(|left, right| left.id.cmp(&right.id));
    }
    merged.validate()?;
    Ok(merged)
}

/// The delta shape, written for a prompt rather than for a parser: short enough to sit in
/// every intent turn and explicit about the two rules a model gets wrong (invented question
/// ids, and treating the delta as a second spec).
#[must_use]
pub fn delta_schema_excerpt() -> String {
    format!(
        r#"Return ONLY this JSON object. It is a correction to the plan you were shown, not a new plan.
{{
  "format": "{GAME_SPEC_DELTA_FORMAT}",
  "title": "string, optional",
  "genre": {{"value":"string","confidence_bps":0-10000,"certainty":"certain|assumed|ambiguous","alternatives":["only when ambiguous, 2+"]}},
  "player_loop_add": [ same shape as genre ],
  "mechanics_add":  [ {{"id":"req_snake_case","statement":"string","confidence_bps":0-10000,"depends_on":["req_..."]}} ],
  "world_add":      [ same shape as mechanics_add ],
  "actors_add":     [ same shape as mechanics_add ],
  "ui_add":         [ same shape as mechanics_add ],
  "answers":        [ {{"question_id":"id from the plan card","answer":"one of that question's options"}} ],
  "remove_ids":     [ "req_... already on the plan card" ]
}}
Rules: every field is optional and every list may be empty. Ids are lowercase letters, digits
and underscores only. At most {MAX_DELTA_ITEMS} changes in total. Never invent a question id
or an option; if the answer is not on the card, leave it out. Never remove a requirement an
acceptance mechanic tests. Say nothing outside the JSON object."#
    )
}

fn requirement_confidence(drafted: &IntentDraft, requirement_id: &str) -> u16 {
    let slot = match requirement_id {
        REQ_RULES => Some(IntentSlot::Win),
        REQ_PLAYER | REQ_CAMERA => Some(IntentSlot::Perspective),
        _ => None,
    };
    let stated = slot
        .and_then(|slot| drafted.fact(slot))
        .is_some_and(|fact| fact.certainty == FactCertainty::Certain);
    if stated {
        CERTAIN_CONFIDENCE_BPS
    } else {
        ASSUMED_CONFIDENCE_BPS
    }
}

fn depends_on(requirement_id: &str, bucket: SpecBucket) -> Vec<String> {
    match requirement_id {
        REQ_PLAYER | REQ_LEVEL | REQ_RULES => Vec::new(),
        REQ_CAMERA => vec![REQ_PLAYER.to_owned()],
        REQ_HUD => vec![REQ_RULES.to_owned()],
        _ => match bucket {
            SpecBucket::Mechanics => vec![REQ_PLAYER.to_owned()],
            SpecBucket::World | SpecBucket::Actors => vec![REQ_LEVEL.to_owned()],
            SpecBucket::Ui => vec![REQ_HUD.to_owned()],
        },
    }
}

/// Counts the prompt gave that this capability actually has a knob for. `"10 feathers"`
/// becomes `collect_target = 10` only because the collectible preset exposes that property.
fn count_notes(drafted: &IntentDraft, capability_id: &str) -> Vec<String> {
    let mut notes = Vec::new();
    for count in &drafted.counts {
        let Some(entry) = catalog::nouns()
            .iter()
            .find(|entry| entry.words.contains(&count.noun.as_str()))
        else {
            continue;
        };
        if catalog::preset_property(capability_id, entry.property).is_some() {
            notes.push(format!("{} = {}", entry.property, count.n));
        }
    }
    notes
}

fn title_for(drafted: &IntentDraft, pack: &Archetype) -> String {
    match drafted.fact(IntentSlot::Setting) {
        Some(setting) => format!("{} {}", title_case(&setting.value), pack.name),
        None => pack.name.clone(),
    }
}

fn title_case(value: &str) -> String {
    let mut chars = value.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

fn schema(message: String, hint: String) -> EngineError {
    EngineError::Schema(message, Some(hint))
}

#[cfg(test)]
mod tests {
    use super::{
        delta_schema_excerpt, merge, spec_from_draft, GameSpecDelta, QuestionAnswer,
        GAME_SPEC_DELTA_FORMAT, MAX_DELTA_ITEMS,
    };
    use crate::game_spec::{GameRequirement, GameSpec, QuestionImpact};
    use crate::intent::archetype::Archetype;
    use crate::intent::draft::draft;

    const GOLDEN: &str = "a cozy third-person exploration game with jump-and-glide, low-poly \
         islands, collect 10 feathers to unlock the lighthouse";

    fn golden_spec() -> GameSpec {
        let drafted = draft(GOLDEN);
        let pack = Archetype::find("exploration").expect("exploration pack");
        spec_from_draft(&drafted, pack)
    }

    fn empty_delta() -> GameSpecDelta {
        GameSpecDelta {
            format: GAME_SPEC_DELTA_FORMAT.to_owned(),
            ..GameSpecDelta::default()
        }
    }

    #[test]
    fn every_archetype_expands_into_a_valid_spec_with_no_prompt_at_all() {
        for pack in crate::intent::archetype::builtin() {
            let spec = spec_from_draft(&draft(""), pack);
            spec.validate()
                .unwrap_or_else(|error| panic!("{} expands invalid: {error}", pack.id));
            assert_eq!(spec.title, pack.name);
        }
    }

    #[test]
    fn the_golden_prompt_expands_into_a_titled_valid_spec() {
        let spec = golden_spec();
        spec.validate().expect("golden spec validates");
        assert_eq!(spec.title, "Island Exploration");
        assert_eq!(spec.genre.value, "exploration");
        assert!(spec
            .constraints
            .quality
            .iter()
            .any(|fact| fact.value == "low-poly"));
    }

    #[test]
    fn a_stated_win_condition_answers_the_critical_question_and_leaves_two_open() {
        let spec = golden_spec();
        let unlock = spec
            .open_questions
            .iter()
            .find(|question| question.id == "unlock_condition")
            .expect("unlock question present");
        assert_eq!(unlock.impact, QuestionImpact::Critical);
        assert_eq!(unlock.resolved.as_deref(), Some("collect-n"));
        let unresolved = spec
            .open_questions
            .iter()
            .filter(|question| question.resolved.is_none())
            .count();
        assert_eq!(unresolved, 2);
    }

    #[test]
    fn a_high_question_puts_its_archetype_default_first_in_the_options() {
        let spec = golden_spec();
        let traversal = spec
            .open_questions
            .iter()
            .find(|question| question.id == "traversal_ability")
            .expect("traversal question present");
        assert_eq!(traversal.options.first().map(String::as_str), Some("glide"));
    }

    #[test]
    fn a_prompt_count_lands_on_the_preset_knob_that_exists_for_it() {
        let spec = golden_spec();
        assert!(spec
            .requirements()
            .iter()
            .any(|requirement| requirement.statement.contains("collect_target = 10")));
    }

    #[test]
    fn an_unknown_delta_format_blocks() {
        let spec = golden_spec();
        let delta = GameSpecDelta {
            format: "bhippi-game-spec-delta@2".to_owned(),
            ..GameSpecDelta::default()
        };
        let error = merge(&spec, &delta).expect_err("major 2 blocks");
        assert!(error.hint().is_some_and(|hint| hint.contains("block")));
    }

    #[test]
    fn an_oversized_delta_blocks_with_the_limit_in_the_hint() {
        let spec = golden_spec();
        let delta = GameSpecDelta {
            player_loop_add: (0..=MAX_DELTA_ITEMS)
                .map(|index| crate::game_spec::SpecFact {
                    value: format!("step {index}"),
                    confidence_bps: 6_000,
                    certainty: crate::game_spec::FactCertainty::Assumed,
                    alternatives: Vec::new(),
                })
                .collect(),
            ..empty_delta()
        };
        let error = merge(&spec, &delta).expect_err("oversized delta blocks");
        assert!(error.to_string().contains(&MAX_DELTA_ITEMS.to_string()));
    }

    #[test]
    fn an_unknown_question_id_blocks_and_a_wrong_option_lists_the_real_ones() {
        let spec = golden_spec();
        let unknown = GameSpecDelta {
            answers: vec![QuestionAnswer {
                question_id: "colour_palette".to_owned(),
                answer: "warm".to_owned(),
            }],
            ..empty_delta()
        };
        let error = merge(&spec, &unknown).expect_err("unknown question blocks");
        assert!(error.to_string().contains("colour_palette"));

        let wrong = GameSpecDelta {
            answers: vec![QuestionAnswer {
                question_id: "traversal_ability".to_owned(),
                answer: "jetpack".to_owned(),
            }],
            ..empty_delta()
        };
        let error = merge(&spec, &wrong).expect_err("unknown option blocks");
        assert!(error.hint().is_some_and(|hint| hint.contains("glide")));
    }

    #[test]
    fn removing_a_requirement_an_acceptance_mechanic_tests_blocks() {
        let spec = golden_spec();
        let delta = GameSpecDelta {
            remove_ids: vec!["req_ability_glide".to_owned()],
            ..empty_delta()
        };
        let error = merge(&spec, &delta).expect_err("tested requirement is protected");
        assert!(error.to_string().contains("req_ability_glide"));

        let unknown = GameSpecDelta {
            remove_ids: vec!["req_ability_jetpack".to_owned()],
            ..empty_delta()
        };
        let error = merge(&spec, &unknown).expect_err("unknown removal blocks");
        assert!(error.to_string().contains("unknown requirement"));
    }

    #[test]
    fn a_legal_delta_adds_answers_and_reorders_deterministically() {
        let spec = golden_spec();
        let delta = GameSpecDelta {
            title: Some("Feather Run".to_owned()),
            mechanics_add: vec![GameRequirement {
                id: "req_ability_dash".to_owned(),
                statement: "A ground dash for crossing short gaps quickly.".to_owned(),
                confidence_bps: 7_000,
                depends_on: vec!["req_player".to_owned()],
                preferred_capabilities: Vec::new(),
                maturity: crate::registry::MaturityRequirement::default(),
            }],
            answers: vec![QuestionAnswer {
                question_id: "world_scale".to_owned(),
                answer: "one-island".to_owned(),
            }],
            ..empty_delta()
        };
        let merged = merge(&spec, &delta).expect("legal delta merges");
        assert_eq!(merged.title, "Feather Run");
        assert!(merged
            .mechanics
            .iter()
            .any(|requirement| requirement.id == "req_ability_dash"));
        let ids = merged
            .mechanics
            .iter()
            .map(|requirement| requirement.id.as_str())
            .collect::<Vec<_>>();
        let mut sorted = ids.clone();
        sorted.sort_unstable();
        assert_eq!(ids, sorted);
        assert_eq!(
            merged
                .open_questions
                .iter()
                .find(|question| question.id == "world_scale")
                .and_then(|question| question.resolved.as_deref()),
            Some("one-island")
        );
        assert_eq!(merge(&spec, &delta).ok(), Some(merged));
    }

    #[test]
    fn adding_a_requirement_that_already_exists_replaces_it_rather_than_duplicating() {
        let spec = golden_spec();
        let delta = GameSpecDelta {
            mechanics_add: vec![GameRequirement {
                id: "req_rules".to_owned(),
                statement: "Collect every feather, then ring the lighthouse bell.".to_owned(),
                confidence_bps: 9_000,
                depends_on: Vec::new(),
                preferred_capabilities: Vec::new(),
                maturity: crate::registry::MaturityRequirement::default(),
            }],
            ..empty_delta()
        };
        let merged = merge(&spec, &delta).expect("replacement merges");
        let matches = merged
            .mechanics
            .iter()
            .filter(|requirement| requirement.id == "req_rules")
            .collect::<Vec<_>>();
        assert_eq!(matches.len(), 1);
        assert!(matches[0].statement.contains("lighthouse bell"));
    }

    #[test]
    fn the_schema_excerpt_stays_small_enough_to_sit_in_every_turn() {
        let excerpt = delta_schema_excerpt();
        assert!(excerpt.contains(GAME_SPEC_DELTA_FORMAT));
        assert!(excerpt.contains("question_id"));
        // ~600 output tokens at four characters per token, with headroom.
        assert!(excerpt.len() < 2_400, "excerpt is {} chars", excerpt.len());
    }

    #[test]
    fn a_delta_round_trips_through_json() {
        let delta = GameSpecDelta {
            answers: vec![QuestionAnswer {
                question_id: "world_scale".to_owned(),
                answer: "one-island".to_owned(),
            }],
            ..empty_delta()
        };
        let text = serde_json::to_string(&delta).expect("delta serialises");
        assert_eq!(GameSpecDelta::parse(&text).expect("delta parses"), delta);
    }
}
