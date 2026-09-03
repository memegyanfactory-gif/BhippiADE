//! The prompt corpus: sixty genre prompts plus six that are not games.
//!
//! The fixture is the contract. It says, for each prompt, which archetype must win, what
//! perspective and dimension the draft must read, which facts and counts must be present, and
//! how many archetype questions are still open once the spec is expanded. A change to the
//! keyword tables that quietly re-genres a prompt fails here rather than in a user's project.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use bhippi_engine::game_spec::FactCertainty;
use bhippi_engine::intent::archetype::{self, Archetype};
use bhippi_engine::intent::delta::spec_from_draft;
use bhippi_engine::intent::draft::{draft, IntentDraft, IntentSlot};
use bhippi_engine::intent::questions::{plan_readiness, unresolved, Readiness};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::PathBuf;

const MIN_GENRE_CASES: usize = 60;
const MIN_OFF_ARCHETYPE_CASES: usize = 6;
const MIN_CASES_PER_ARCHETYPE: usize = 6;

#[derive(Debug, Deserialize)]
struct Corpus {
    format: String,
    cases: Vec<Case>,
}

#[derive(Debug, Deserialize)]
struct Case {
    id: String,
    prompt: String,
    archetype: Option<String>,
    perspective: Option<String>,
    dimension: Option<String>,
    facts: Vec<ExpectedFact>,
    counts: Vec<ExpectedCount>,
    unresolved_questions: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct ExpectedFact {
    slot: IntentSlot,
    value: String,
    certainty: Option<FactCertainty>,
}

#[derive(Debug, Deserialize, Eq, PartialEq)]
struct ExpectedCount {
    noun: String,
    n: u32,
}

fn corpus() -> Corpus {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/intent/prompts-v1.json");
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("committed corpus at {}: {error}", path.display()));
    serde_json::from_str(&text).expect("corpus parses")
}

fn slot_value(drafted: &IntentDraft, slot: IntentSlot) -> Option<String> {
    drafted.fact(slot).map(|fact| fact.value.clone())
}

#[test]
fn the_corpus_covers_every_archetype_and_a_real_off_archetype_tail() {
    let corpus = corpus();
    assert_eq!(corpus.format, "bhippi-intent-prompts@1");
    let mut ids = BTreeMap::new();
    for case in &corpus.cases {
        assert!(
            ids.insert(case.id.clone(), ()).is_none(),
            "duplicate case id {}",
            case.id
        );
    }

    let mut per_archetype: BTreeMap<&str, usize> = BTreeMap::new();
    let mut off = 0;
    for case in &corpus.cases {
        match case.archetype.as_deref() {
            Some(id) => *per_archetype.entry(id).or_default() += 1,
            None => off += 1,
        }
    }
    assert!(
        off >= MIN_OFF_ARCHETYPE_CASES,
        "only {off} off-archetype prompts"
    );
    assert!(
        corpus.cases.len() - off >= MIN_GENRE_CASES,
        "only {} genre prompts",
        corpus.cases.len() - off
    );
    for pack in archetype::builtin() {
        let seen = per_archetype.get(pack.id.as_str()).copied().unwrap_or(0);
        assert!(
            seen >= MIN_CASES_PER_ARCHETYPE,
            "{} has only {seen} prompts",
            pack.id
        );
    }
}

#[test]
fn every_prompt_drafts_the_way_the_corpus_says() {
    for case in corpus().cases {
        let drafted = draft(&case.prompt);
        let matched = drafted.archetype.as_ref().map(|hit| hit.id.clone());
        assert_eq!(
            matched.as_deref(),
            case.archetype.as_deref(),
            "{}: archetype (candidates {:?})",
            case.id,
            drafted
                .candidates
                .iter()
                .map(|hit| (hit.id.as_str(), hit.score_bps))
                .collect::<Vec<_>>()
        );
        assert_eq!(
            slot_value(&drafted, IntentSlot::Perspective).as_deref(),
            case.perspective.as_deref(),
            "{}: perspective",
            case.id
        );
        assert_eq!(
            slot_value(&drafted, IntentSlot::Dimension).as_deref(),
            case.dimension.as_deref(),
            "{}: dimension",
            case.id
        );

        for expected in &case.facts {
            let held = drafted
                .facts_for(expected.slot)
                .into_iter()
                .find(|fact| fact.value == expected.value)
                .unwrap_or_else(|| {
                    panic!(
                        "{}: no {:?} fact {:?}; drafted {:?}",
                        case.id,
                        expected.slot,
                        expected.value,
                        drafted
                            .facts_for(expected.slot)
                            .into_iter()
                            .map(|fact| fact.value.clone())
                            .collect::<Vec<_>>()
                    )
                });
            if let Some(certainty) = expected.certainty {
                assert_eq!(
                    held.certainty, certainty,
                    "{}: certainty of {:?}",
                    case.id, expected.value
                );
            }
        }

        let counts = drafted
            .counts
            .iter()
            .map(|count| ExpectedCount {
                noun: count.noun.clone(),
                n: count.n,
            })
            .collect::<Vec<_>>();
        assert_eq!(counts, case.counts, "{}: counts", case.id);
    }
}

#[test]
fn every_matched_prompt_expands_into_a_valid_spec_with_the_expected_open_questions() {
    for case in corpus().cases {
        let Some(pack_id) = case.archetype.as_deref() else {
            assert!(
                case.unresolved_questions.is_none(),
                "{}: an unmatched prompt cannot expand a spec",
                case.id
            );
            continue;
        };
        let pack = Archetype::find(pack_id).expect("corpus names a built-in pack");
        let drafted = draft(&case.prompt);
        let spec = spec_from_draft(&drafted, pack);
        spec.validate()
            .unwrap_or_else(|error| panic!("{}: spec invalid: {error}", case.id));

        let open = unresolved(&spec).len();
        assert_eq!(
            Some(open),
            case.unresolved_questions,
            "{}: open questions ({:?})",
            case.id,
            unresolved(&spec)
                .iter()
                .map(|question| question.id.as_str())
                .collect::<Vec<_>>()
        );
        assert!(
            open <= pack.questions.len(),
            "{}: more open questions than the pack asks",
            case.id
        );
    }
}

#[test]
fn the_docs_16_golden_prompt_lands_exactly_as_the_plan_describes() {
    let prompt = "a cozy third-person exploration game with jump-and-glide, low-poly islands, \
         collect 10 feathers to unlock the lighthouse";
    let drafted = draft(prompt);
    assert_eq!(
        drafted.archetype.as_ref().map(|hit| hit.id.as_str()),
        Some("exploration")
    );
    assert_eq!(
        slot_value(&drafted, IntentSlot::Perspective).as_deref(),
        Some("third_person")
    );
    assert_eq!(
        slot_value(&drafted, IntentSlot::Dimension).as_deref(),
        Some("three_d")
    );
    let styles = drafted
        .facts_for(IntentSlot::ArtStyle)
        .into_iter()
        .map(|fact| fact.value.clone())
        .collect::<Vec<_>>();
    assert!(styles.contains(&"low-poly".to_owned()));
    assert!(styles.contains(&"cozy".to_owned()));
    assert_eq!(drafted.count("feathers"), Some(10));

    let pack = Archetype::find("exploration").expect("exploration ships");
    let spec = spec_from_draft(&drafted, pack);
    spec.validate().expect("golden spec validates");
    assert_eq!(spec.title, "Island Exploration");
    assert_eq!(
        spec.open_questions
            .iter()
            .find(|question| question.id == "unlock_condition")
            .and_then(|question| question.resolved.as_deref()),
        Some("collect-n"),
        "the unlock question is answered by the prompt itself"
    );
    assert!(
        unresolved(&spec).len() <= 2,
        "the plan promises at most two open questions"
    );
    assert!(
        matches!(plan_readiness(&spec), Readiness::ReadyWithDefaults { .. }),
        "no Critical decision is left open"
    );
}

#[test]
fn an_off_archetype_prompt_never_pretends_to_be_a_game() {
    for case in corpus()
        .cases
        .iter()
        .filter(|case| case.archetype.is_none())
    {
        let drafted = draft(&case.prompt);
        assert!(drafted.archetype.is_none(), "{}: matched a genre", case.id);
        assert!(
            drafted
                .unresolved
                .iter()
                .any(|slot| slot.slot == IntentSlot::Genre),
            "{}: should report the genre as unresolved",
            case.id
        );
    }
}

#[test]
fn drafting_the_whole_corpus_twice_gives_byte_identical_results() {
    for case in corpus().cases {
        let once = serde_json::to_string(&draft(&case.prompt)).expect("draft serialises");
        let twice = serde_json::to_string(&draft(&case.prompt)).expect("draft serialises");
        assert_eq!(once, twice, "{}: drafting is not deterministic", case.id);
    }
}
