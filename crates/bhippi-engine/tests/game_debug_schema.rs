#![allow(clippy::expect_used)]

use bhippi_engine::game_debug::{GameDebugFinding, GameDebugReport};

fn fixture() -> String {
    std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/engine/quality/game-debug-report-v1.json"),
    )
    .expect("game-debug golden")
    .trim_end()
    .to_owned()
}

fn finding(code: &str, severity: &str, address: &str) -> GameDebugFinding {
    GameDebugFinding {
        code: code.to_owned(),
        severity: severity.to_owned(),
        stage: "06_inspect".to_owned(),
        address: address.to_owned(),
        message: "observed defect".to_owned(),
        evidence: "deterministic evidence".to_owned(),
        reproduction: "run /gamedebug quick".to_owned(),
        repair: "repair the addressed game document".to_owned(),
    }
}

#[test]
fn v1_report_golden_round_trips_byte_for_byte() {
    let expected = fixture();
    let report = GameDebugReport::parse(&expected).expect("golden parses");
    assert_eq!(report.dump().expect("golden dumps"), expected);
}

#[test]
fn report_rejects_noncanonical_or_contradictory_evidence() {
    let report = GameDebugReport::parse(&fixture()).expect("golden parses");

    let mut reordered = report.clone();
    reordered.stages.swap(0, 1);
    assert!(reordered.validate().is_err());

    let mut unsorted = report.clone();
    unsorted.findings = vec![
        finding("BHP-GD-399", "warning", "z"),
        finding("BHP-GD-301", "blocker", "a"),
    ];
    unsorted.outcome = "failed".to_owned();
    assert!(unsorted.validate().is_err());

    let mut changed = report;
    changed.authored_tree_after =
        "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".to_owned();
    assert!(changed.validate().is_err());
}
