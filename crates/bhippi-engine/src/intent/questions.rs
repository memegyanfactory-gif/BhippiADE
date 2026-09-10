//! Open-question policy: which decisions block a build and which take a default.
//!
//! The rule is the plan's (`docs/16 §5.2` step 3), stated once here so every surface agrees:
//! a **Critical** question the user has not answered blocks the build, and a **High** question
//! takes the archetype default and is flagged on the plan card. Nothing else is allowed to
//! decide it — an assumption that quietly closes a Critical decision is exactly the failure
//! the plan card exists to prevent.
//!
//! `OpenQuestion` has no default field of its own, so the expansion in
//! [`crate::intent::delta::spec_from_draft`] puts the archetype default first in `options`.
//! [`question_default`] is the only place that convention is read.

use crate::error::{EngineError, Result};
use crate::game_spec::{GameSpec, OpenQuestion, QuestionImpact};
use serde::{Deserialize, Serialize};
use specta::Type;

/// A High question and the answer it will take if nobody chooses.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct QuestionDefault {
    pub question_id: String,
    pub default: String,
}

/// Whether the plan may proceed to a build.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum Readiness {
    /// Every material decision is made.
    Ready,
    /// At least one Critical decision is open. The build cannot start.
    BlockedByCritical { question_ids: Vec<String> },
    /// Only High decisions are open; these defaults will be applied and flagged.
    ReadyWithDefaults { defaults: Vec<QuestionDefault> },
}

/// The questions nobody has answered yet, in plan-card order.
#[must_use]
pub fn unresolved(spec: &GameSpec) -> Vec<&OpenQuestion> {
    spec.open_questions
        .iter()
        .filter(|question| question.resolved.is_none())
        .collect()
}

/// The default answer for a question: by convention the first option, which
/// `spec_from_draft` fills from the archetype.
#[must_use]
pub fn question_default(question: &OpenQuestion) -> Option<&str> {
    question.options.first().map(String::as_str)
}

/// Read the plan's readiness. Critical wins over High: a plan with both open is blocked, and
/// the High defaults are not reported, because answering the Critical one may change them.
#[must_use]
pub fn plan_readiness(spec: &GameSpec) -> Readiness {
    let open = unresolved(spec);
    let blocking = open
        .iter()
        .filter(|question| question.impact == QuestionImpact::Critical)
        .map(|question| question.id.clone())
        .collect::<Vec<_>>();
    if !blocking.is_empty() {
        return Readiness::BlockedByCritical {
            question_ids: blocking,
        };
    }
    let defaults = open
        .iter()
        .filter_map(|question| {
            question_default(question).map(|default| QuestionDefault {
                question_id: question.id.clone(),
                default: default.to_owned(),
            })
        })
        .collect::<Vec<_>>();
    if defaults.is_empty() {
        Readiness::Ready
    } else {
        Readiness::ReadyWithDefaults { defaults }
    }
}

/// Record an answer and re-validate. The spec is returned changed only if the answer is one
/// the question actually offers; free text is accepted only where a question offers no
/// closed option set.
pub fn answer(spec: &GameSpec, question_id: &str, chosen: &str) -> Result<GameSpec> {
    let mut answered = spec.clone();
    let question = answered
        .open_questions
        .iter_mut()
        .find(|question| question.id == question_id)
        .ok_or_else(|| {
            EngineError::NotFound(
                format!("open question {question_id:?}"),
                Some("Answer one of the question ids the plan card lists.".to_owned()),
            )
        })?;
    if chosen.trim().is_empty() {
        return Err(EngineError::Schema(
            format!("question {question_id:?} was answered with nothing"),
            Some("Choose an option, or say what you want in words.".to_owned()),
        ));
    }
    if !question.options.is_empty() && !question.options.contains(&chosen.to_owned()) {
        return Err(EngineError::Schema(
            format!("question {question_id:?} has no option {chosen:?}"),
            Some(format!("Choose one of: {}.", question.options.join(", "))),
        ));
    }
    question.resolved = Some(chosen.to_owned());
    answered.validate()?;
    Ok(answered)
}

#[cfg(test)]
mod tests {
    use super::{answer, plan_readiness, question_default, unresolved, Readiness};
    use crate::game_spec::GameSpec;
    use crate::intent::archetype::Archetype;
    use crate::intent::delta::spec_from_draft;
    use crate::intent::draft::draft;

    fn spec_for(prompt: &str, pack_id: &str) -> GameSpec {
        let pack = Archetype::find(pack_id).expect("pack exists");
        spec_from_draft(&draft(prompt), pack)
    }

    fn answer_all_critical(mut spec: GameSpec) -> GameSpec {
        loop {
            let Readiness::BlockedByCritical { question_ids } = plan_readiness(&spec) else {
                return spec;
            };
            let id = question_ids.first().cloned().expect("blocked lists an id");
            let choice = spec
                .open_questions
                .iter()
                .find(|question| question.id == id)
                .and_then(|question| question.options.first())
                .cloned()
                .expect("critical question offers options");
            spec = answer(&spec, &id, &choice).expect("valid option applies");
        }
    }

    #[test]
    fn an_unanswered_critical_question_blocks_every_archetype() {
        for pack in crate::intent::archetype::builtin() {
            let spec = spec_from_draft(&draft(""), pack);
            let Readiness::BlockedByCritical { question_ids } = plan_readiness(&spec) else {
                panic!("{} should block on its Critical decision", pack.id);
            };
            assert!(!question_ids.is_empty());
        }
    }

    #[test]
    fn once_the_critical_questions_are_answered_the_high_ones_supply_defaults() {
        let spec = answer_all_critical(spec_for("", "racing_kart"));
        let Readiness::ReadyWithDefaults { defaults } = plan_readiness(&spec) else {
            panic!("only High questions should remain");
        };
        let racers = defaults
            .iter()
            .find(|entry| entry.question_id == "ai_racers")
            .expect("the AI-racer count defaults");
        assert_eq!(racers.default, "three");
    }

    #[test]
    fn answering_everything_reads_ready() {
        let mut spec = answer_all_critical(spec_for("", "puzzle_physics"));
        while let Some(question) = unresolved(&spec).first() {
            let id = question.id.clone();
            let choice = question_default(question)
                .expect("a High question has a default")
                .to_owned();
            spec = answer(&spec, &id, &choice).expect("default applies");
        }
        assert_eq!(plan_readiness(&spec), Readiness::Ready);
    }

    #[test]
    fn an_unknown_question_or_an_unoffered_option_is_refused_with_a_hint() {
        let spec = spec_for("", "survival");
        let error = answer(&spec, "colour_grade", "warm").expect_err("unknown id is refused");
        assert!(error.to_string().contains("colour_grade"));
        assert!(error.hint().is_some());

        let error = answer(&spec, "run_goal", "vibes").expect_err("unoffered option is refused");
        assert!(error
            .hint()
            .is_some_and(|hint| hint.contains("survive-time")));

        let error = answer(&spec, "run_goal", "  ").expect_err("an empty answer is refused");
        assert!(error.to_string().contains("nothing"));
    }

    #[test]
    fn a_prompt_that_states_the_decision_answers_it_without_asking() {
        let spec = spec_for(
            "a 2d platformer with three lives and a forest theme",
            "platformer_2d",
        );
        let life_model = spec
            .open_questions
            .iter()
            .find(|question| question.id == "life_model")
            .expect("life model asked");
        assert_eq!(life_model.resolved.as_deref(), Some("lives"));
        assert!(matches!(
            plan_readiness(&spec),
            Readiness::ReadyWithDefaults { .. }
        ));
    }

    #[test]
    fn readiness_serialises_with_a_state_tag_for_the_plan_card() {
        let json = serde_json::to_string(&Readiness::BlockedByCritical {
            question_ids: vec!["life_model".to_owned()],
        })
        .expect("readiness serialises");
        assert!(json.contains("\"state\":\"blocked_by_critical\""));
    }
}
