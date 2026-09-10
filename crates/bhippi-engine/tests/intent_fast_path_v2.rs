//! A test states its preconditions with `unwrap`/`expect`: a panic here is a failing
//! test, not a crashed app. The workspace `deny` stands everywhere else.
#![allow(clippy::expect_used, clippy::unwrap_used)]

//! Tests for Phase 8 GAD-133: 100-utterance follow-up corpus and fast-path share KPI.

#![allow(dead_code, unused_imports)]

use bhippi_engine::intent::fast_path::{
    propose, FastPathContext, FAST_PATH_APPLY_BPS, FAST_PATH_CONFIRM_BPS,
};
use serde::Deserialize;
use std::path::PathBuf;

const EXPECTED_V2_CASES: usize = 100;
const MINIMUM_FAST_PATH_SHARE_PERCENT: f64 = 60.0;

#[derive(Debug, Deserialize)]
struct FastPathCorpus {
    format: String,
    note: String,
    context: FastPathContext,
    cases: Vec<CorpusCase>,
}

#[derive(Debug, Deserialize)]
struct CorpusCase {
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

fn corpus_v2() -> FastPathCorpus {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/intent/fastpath-v2.json");
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("committed v2 corpus at {}: {error}", path.display()));
    serde_json::from_str(&text).expect("corpus parses")
}

#[test]
fn gad_133_hundred_utterances_corpus_matches_proposals() {
    let corpus = corpus_v2();
    assert_eq!(corpus.format, "bhippi-intent-fastpath@2");
    assert_eq!(corpus.cases.len(), EXPECTED_V2_CASES);

    let mut apply_count = 0;
    let mut confirm_count = 0;
    let mut none_count = 0;

    for case in &corpus.cases {
        let proposal = propose(&case.utterance, &corpus.context);
        match case.band {
            Band::Apply => {
                apply_count += 1;
                let prop =
                    proposal.unwrap_or_else(|| panic!("'{}' should propose", case.utterance));
                assert!(
                    prop.confidence_bps >= FAST_PATH_APPLY_BPS,
                    "'{}' confidence {} < apply threshold {}",
                    case.utterance,
                    prop.confidence_bps,
                    FAST_PATH_APPLY_BPS
                );
                assert_eq!(prop.target.node_path, case.node_path);
                assert_eq!(prop.target.preset_id, case.preset_id);
                if let Some(prop_name) = &case.property {
                    assert_eq!(&prop.target.property, prop_name);
                }
            }
            Band::Confirm => {
                confirm_count += 1;
                let prop =
                    proposal.unwrap_or_else(|| panic!("'{}' should propose", case.utterance));
                assert!(
                    prop.confidence_bps >= FAST_PATH_CONFIRM_BPS,
                    "'{}' confidence {} < confirm threshold {}",
                    case.utterance,
                    prop.confidence_bps,
                    FAST_PATH_CONFIRM_BPS
                );
            }
            Band::None => {
                none_count += 1;
                assert!(
                    proposal.is_none() || proposal.unwrap().confidence_bps < FAST_PATH_CONFIRM_BPS,
                    "'{}' should decline fast path",
                    case.utterance
                );
            }
        }
    }

    assert_eq!(apply_count + confirm_count + none_count, EXPECTED_V2_CASES);

    // Fast-path share KPI: percentage of requests resolved without a model turn
    let share = (apply_count + confirm_count) as f64 / EXPECTED_V2_CASES as f64 * 100.0;
    assert!(
        share >= MINIMUM_FAST_PATH_SHARE_PERCENT,
        "Fast path share {share:.1}% below minimum target {MINIMUM_FAST_PATH_SHARE_PERCENT}%"
    );
    // 74.0 before the named-node rule: seven of those "applies" were the fast path editing a
    // node the user had not named ("set patroller speed to 4" moved /root/Game/Player). They
    // are model turns now, so the share this corpus records is 67.0.
    assert_eq!(share, 67.0);
}
