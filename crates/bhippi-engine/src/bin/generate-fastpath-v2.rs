//! Generates tests/fixtures/intent/fastpath-v2.json with 100 follow-up utterances and tests fast-path share KPI.
//!
//! Run: `cargo run -p bhippi-engine --bin generate-fastpath-v2`

use bhippi_engine::intent::fast_path::{
    propose, FastPathContext, FastPathOp, FAST_PATH_APPLY_BPS, FAST_PATH_CONFIRM_BPS,
};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Deserialize, Serialize)]
struct FastPathCorpus {
    format: String,
    note: String,
    context: FastPathContext,
    cases: Vec<CorpusCase>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct CorpusCase {
    utterance: String,
    band: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    node_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    preset_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    property: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    op: Option<SerializedOp>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    label: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    candidates: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum SerializedOp {
    Multiply { value: f64 },
    Add { value: f64 },
    SetNumber { value: f64 },
    SetBool { value: bool },
    SetText { value: String },
}

const NEW_UTTERANCES: &[&str] = &[
    // Direct player & enemy parameter tweaks (apply / confirm)
    "make the player speed 10% faster",
    "make the player speed 30% slower",
    "make the jump velocity 7",
    "make the jump velocity 20% higher",
    "increase player jump velocity by 1.5",
    "reduce player jump velocity by 1",
    "make the glide 50% longer",
    "make the glide 10% shorter",
    "set glide time to 5",
    "set glide time to 2",
    "make player gravity 20% stronger",
    "make player gravity 20% weaker",
    "set player gravity to 20",
    "set player gravity to 10",
    "make player dash speed 25",
    "make player dash speed 12",
    "make player dash speed 20% faster",
    "make player dash speed 10% slower",
    "make player acceleration 15",
    "make player acceleration 20% faster",
    "make the chaser speed 5",
    "make the chaser speed 20% faster",
    "make the chaser speed 10% slower",
    "set chaser damage to 15",
    "set chaser damage to 5",
    "make chaser max health 50",
    "make chaser max health 20",
    "make chaser max health 20% higher",
    "set patroller speed to 4",
    "set patroller speed to 2",
    "make patroller speed 20% faster",
    "set patroller attack range to 12",
    "set patroller attack range to 6",
    "make patroller attack range 25% larger",
    "make patroller attack range 20% smaller",
    "set rules collect target to 20",
    "set rules collect target to 5",
    "make collect target 50% higher",
    "set player lives to 5",
    "set player lives to 1",
    "set lap count to 5",
    "set lap count to 2",
    "set checkpoint count to 8",
    "set checkpoint count to 4",
    "set time limit to 180",
    "set time limit to 600",
    "turn the fog on",
    "turn the fog off",
    "turn the rain off",
    // Qualitative / ambiguous tweaks needing confirmation (confirm)
    "speed up the enemies slightly",
    "make the patroller faster",
    "make the chaser stronger",
    // Architectural, creative, or multi-step requests (none -> model turn)
    "add an inventory system with 10 slots",
    "create a dialogue tree with the town elder",
    "implement a day night weather cycle with shadows",
    "change the perspective to first person shooter",
    "add a mini-map in the top right corner",
    "add achievements for collecting all feathers",
    "make the world procedural with perlin noise islands",
    "add sound effects when the player jumps and lands",
];

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let fixture_root = manifest_dir.join("../../tests/fixtures/intent");

    let v1_path = fixture_root.join("fastpath-v1.json");
    let v1_text = std::fs::read_to_string(&v1_path)?;
    let v1_corpus: FastPathCorpus = serde_json::from_str(&v1_text)?;

    println!("v1 cases count: {}", v1_corpus.cases.len());

    let context = &v1_corpus.context;
    let mut total_cases = v1_corpus.cases;

    for &utterance in NEW_UTTERANCES {
        let proposal = propose(utterance, context);
        let case = match proposal {
            Some(prop) if prop.confidence_bps >= FAST_PATH_APPLY_BPS => {
                let op = match prop.op {
                    FastPathOp::Multiply { factor } => SerializedOp::Multiply { value: factor },
                    FastPathOp::Add { amount } => SerializedOp::Add { value: amount },
                    FastPathOp::Set {
                        value: bhippi_engine::intent::fast_path::TscnValueLite::Number { value },
                    } => SerializedOp::SetNumber { value },
                    FastPathOp::Set {
                        value: bhippi_engine::intent::fast_path::TscnValueLite::Bool { value },
                    } => SerializedOp::SetBool { value },
                    FastPathOp::Set {
                        value: bhippi_engine::intent::fast_path::TscnValueLite::Text { value },
                    } => SerializedOp::SetText { value },
                };
                CorpusCase {
                    utterance: utterance.to_owned(),
                    band: "apply".to_owned(),
                    node_path: prop.target.node_path,
                    preset_id: prop.target.preset_id,
                    property: Some(prop.target.property),
                    op: Some(op),
                    label: Some(prop.label),
                    candidates: prop.candidates,
                }
            }
            Some(prop) if prop.confidence_bps >= FAST_PATH_CONFIRM_BPS => {
                let op = match prop.op {
                    FastPathOp::Multiply { factor } => SerializedOp::Multiply { value: factor },
                    FastPathOp::Add { amount } => SerializedOp::Add { value: amount },
                    FastPathOp::Set {
                        value: bhippi_engine::intent::fast_path::TscnValueLite::Number { value },
                    } => SerializedOp::SetNumber { value },
                    FastPathOp::Set {
                        value: bhippi_engine::intent::fast_path::TscnValueLite::Bool { value },
                    } => SerializedOp::SetBool { value },
                    FastPathOp::Set {
                        value: bhippi_engine::intent::fast_path::TscnValueLite::Text { value },
                    } => SerializedOp::SetText { value },
                };
                CorpusCase {
                    utterance: utterance.to_owned(),
                    band: "confirm".to_owned(),
                    node_path: prop.target.node_path,
                    preset_id: prop.target.preset_id,
                    property: Some(prop.target.property),
                    op: Some(op),
                    label: Some(prop.label),
                    candidates: prop.candidates,
                }
            }
            _ => CorpusCase {
                utterance: utterance.to_owned(),
                band: "none".to_owned(),
                node_path: None,
                preset_id: None,
                property: None,
                op: None,
                label: None,
                candidates: Vec::new(),
            },
        };
        total_cases.push(case);
    }

    println!("Total cases: {}", total_cases.len());
    assert_eq!(
        total_cases.len(),
        100,
        "Corpus v2 must have exactly 100 cases"
    );

    let apply_count = total_cases.iter().filter(|c| c.band == "apply").count();
    let confirm_count = total_cases.iter().filter(|c| c.band == "confirm").count();
    let none_count = total_cases.iter().filter(|c| c.band == "none").count();
    let fast_path_share = (apply_count + confirm_count) as f64 / total_cases.len() as f64 * 100.0;

    println!("KPI Summary:");
    println!("  apply:   {apply_count}");
    println!("  confirm: {confirm_count}");
    println!("  none:    {none_count}");
    println!("  Fast-Path Share KPI: {fast_path_share:.1}%");

    let v2_corpus = FastPathCorpus {
        format: "bhippi-intent-fastpath@2".to_owned(),
        note: format!(
            "One hundred follow-up utterances against one shared project (GAD-133). Fast-path share KPI = {fast_path_share:.1}%."
        ),
        context: v1_corpus.context,
        cases: total_cases,
    };

    let v2_path = fixture_root.join("fastpath-v2.json");
    std::fs::write(&v2_path, serde_json::to_string_pretty(&v2_corpus)?)?;
    println!("wrote {}", v2_path.display());

    Ok(())
}
