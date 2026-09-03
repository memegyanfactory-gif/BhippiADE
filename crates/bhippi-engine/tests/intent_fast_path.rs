//! The follow-up corpus: forty utterances against one shared project.
//!
//! Each case pins the target, the operation and the confidence band. `apply` means the edit
//! lands behind an Undo toast, `confirm` means it needs a chip because the knob is not unique
//! or the qualifier carried no number, and `none` means the fast path declines and the turn
//! belongs to a model. Declining is a first-class outcome: a wrong silent edit costs the user
//! far more than one provider call.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use bhippi_engine::intent::fast_path::{
    propose, FastPathContext, FastPathOp, FastPathProposal, TscnValueLite, FAST_PATH_APPLY_BPS,
    FAST_PATH_CONFIRM_BPS,
};
use serde::Deserialize;
use std::collections::BTreeSet;
use std::path::PathBuf;

const EXPECTED_CASES: usize = 40;
const EPSILON: f64 = 1e-9;

#[derive(Debug, Deserialize)]
struct Corpus {
    format: String,
    context: FastPathContext,
    cases: Vec<Case>,
}

#[derive(Debug, Deserialize)]
struct Case {
    utterance: String,
    band: Band,
    #[serde(default)]
    node_path: Option<String>,
    #[serde(default)]
    preset_id: Option<String>,
    #[serde(default)]
    property: Option<String>,
    #[serde(default)]
    op: Option<ExpectedOp>,
    #[serde(default)]
    label: Option<String>,
    #[serde(default)]
    candidates: Vec<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
enum Band {
    Apply,
    Confirm,
    None,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum ExpectedOp {
    Multiply { value: f64 },
    Add { value: f64 },
    SetNumber { value: f64 },
    SetBool { value: bool },
    SetText { value: String },
}

fn corpus() -> Corpus {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/intent/fastpath-v1.json");
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("committed corpus at {}: {error}", path.display()));
    serde_json::from_str(&text).expect("corpus parses")
}

fn band_of(proposal: &FastPathProposal) -> Band {
    if proposal.applies_without_asking() {
        Band::Apply
    } else {
        Band::Confirm
    }
}

fn assert_op(case: &Case, proposal: &FastPathProposal) {
    let Some(expected) = &case.op else {
        return;
    };
    let close = |left: f64, right: f64| (left - right).abs() < EPSILON;
    match (expected, &proposal.op) {
        (ExpectedOp::Multiply { value }, FastPathOp::Multiply { factor }) => assert!(
            close(*value, *factor),
            "{}: factor {factor} is not {value}",
            case.utterance
        ),
        (ExpectedOp::Add { value }, FastPathOp::Add { amount }) => assert!(
            close(*value, *amount),
            "{}: amount {amount} is not {value}",
            case.utterance
        ),
        (
            ExpectedOp::SetNumber { value },
            FastPathOp::Set {
                value: TscnValueLite::Number { value: held },
            },
        ) => assert!(
            close(*value, *held),
            "{}: set {held} is not {value}",
            case.utterance
        ),
        (
            ExpectedOp::SetBool { value },
            FastPathOp::Set {
                value: TscnValueLite::Bool { value: held },
            },
        ) => assert_eq!(value, held, "{}: bool", case.utterance),
        (
            ExpectedOp::SetText { value },
            FastPathOp::Set {
                value: TscnValueLite::Text { value: held },
            },
        ) => assert_eq!(value, held, "{}: text", case.utterance),
        (expected, held) => {
            panic!("{}: expected {expected:?}, got {held:?}", case.utterance)
        }
    }
}

#[test]
fn the_corpus_is_forty_distinct_utterances_over_one_project() {
    let corpus = corpus();
    assert_eq!(corpus.format, "bhippi-intent-fastpath@1");
    assert_eq!(corpus.cases.len(), EXPECTED_CASES);
    let mut seen = BTreeSet::new();
    for case in &corpus.cases {
        assert!(
            seen.insert(case.utterance.clone()),
            "duplicate utterance {:?}",
            case.utterance
        );
    }
    assert!(
        corpus.cases.iter().any(|case| case.band == Band::None),
        "a corpus with no declines proves nothing about restraint"
    );
    assert!(!corpus.context.nodes.is_empty());
}

#[test]
fn every_utterance_lands_in_the_band_the_corpus_expects() {
    let corpus = corpus();
    for case in &corpus.cases {
        let proposed = propose(&case.utterance, &corpus.context);
        if case.band == Band::None {
            assert!(
                proposed.is_none(),
                "{}: expected no proposal, got {proposed:?}",
                case.utterance
            );
            continue;
        }
        let proposal =
            proposed.unwrap_or_else(|| panic!("{}: expected a proposal", case.utterance));
        assert_eq!(band_of(&proposal), case.band, "{}: band", case.utterance);
        assert_eq!(
            proposal.target.property,
            case.property.clone().unwrap_or_default(),
            "{}: property",
            case.utterance
        );
        assert_eq!(
            proposal.target.node_path, case.node_path,
            "{}: node path",
            case.utterance
        );
        assert_eq!(
            proposal.target.preset_id, case.preset_id,
            "{}: preset id",
            case.utterance
        );
        assert_op(case, &proposal);
        if !case.candidates.is_empty() {
            assert_eq!(
                proposal.candidates, case.candidates,
                "{}: candidates",
                case.utterance
            );
        }
        if let Some(label) = &case.label {
            assert_eq!(&proposal.label, label, "{}: label", case.utterance);
        }
        assert!(!proposal.rationale.is_empty());
    }
}

#[test]
fn the_confidence_bands_agree_with_their_thresholds() {
    let corpus = corpus();
    for case in &corpus.cases {
        let Some(proposal) = propose(&case.utterance, &corpus.context) else {
            continue;
        };
        assert!(
            proposal.confidence_bps >= FAST_PATH_CONFIRM_BPS,
            "{}: a proposal below the confirm floor should not exist",
            case.utterance
        );
        match case.band {
            Band::Apply => {
                assert!(proposal.confidence_bps >= FAST_PATH_APPLY_BPS);
                assert!(!proposal.needs_choice());
                assert!(proposal.candidates.is_empty());
            }
            Band::Confirm => assert!(
                proposal.confidence_bps < FAST_PATH_APPLY_BPS || proposal.needs_choice(),
                "{}: confirm needs either low confidence or an open choice",
                case.utterance
            ),
            Band::None => unreachable!("declines produce no proposal"),
        }
    }
}

#[test]
fn a_proposal_always_names_exactly_one_target_or_asks_which() {
    let corpus = corpus();
    for case in &corpus.cases {
        let Some(proposal) = propose(&case.utterance, &corpus.context) else {
            continue;
        };
        let named = usize::from(proposal.target.node_path.is_some())
            + usize::from(proposal.target.preset_id.is_some());
        assert!(named <= 1, "{}: two targets at once", case.utterance);
        assert_eq!(
            named == 0,
            proposal.needs_choice(),
            "{}: an unnamed target must ask",
            case.utterance
        );
        if proposal.needs_choice() {
            assert!(
                proposal.candidates.len() > 1,
                "{}: asking without candidates is useless",
                case.utterance
            );
        }
    }
}

#[test]
fn proposing_twice_gives_the_same_answer() {
    let corpus = corpus();
    for case in &corpus.cases {
        let once = propose(&case.utterance, &corpus.context);
        let twice = propose(&case.utterance, &corpus.context);
        assert_eq!(once, twice, "{}: not deterministic", case.utterance);
    }
}

#[test]
fn the_docs_16_golden_iteration_applies_with_no_model_call() {
    let corpus = corpus();
    let proposal = propose("make the glide 20% longer", &corpus.context)
        .expect("the plan's own iteration example");
    assert!(matches!(
        proposal.op,
        FastPathOp::Multiply { factor } if (factor - 1.2).abs() < EPSILON
    ));
    assert_eq!(proposal.target.property, "glide_time");
    assert_eq!(
        proposal.target.node_path.as_deref(),
        Some("/root/Game/Player")
    );
    assert!(proposal.applies_without_asking());
    assert_eq!(proposal.label, "Set glide_time 3 → 3.6");
}
