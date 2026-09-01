//! Offline golden transcript for the bounded build → verify → repair loop (ENG-185).
//!
//! This deliberately uses no provider or network. It feeds the exact tagged transcript
//! through the production stream scanner and applies its writes through EngineSessions,
//! proving that a real rejection becomes one corrected, undoable transaction before the
//! observation requests are accepted as verification.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use bhippi_app::engine::bridge::{EngineCall, EngineCallScanner};
use bhippi_app::engine::resolve_batch_step;
use bhippi_app::engine::session::{BatchRequest, EngineSessions};
use bhippi_engine::scaffold;
use bhippi_types::{EngineActor, EntityId, ENGINE_AUTONOMY_MAX_ROUNDS};
use serde_json::Value;
use std::path::{Path, PathBuf};

const LEVEL: &str = "assets/scenes/level_01.bscn.json";

fn temp_game() -> PathBuf {
    let root = std::env::temp_dir().join(format!("bhippi-autonomy-{}", EntityId::new()));
    std::fs::create_dir_all(root.join("assets/scenes")).expect("scene folder");
    std::fs::write(
        root.join(LEVEL),
        scaffold::starter_scene().dump().expect("starter scene"),
    )
    .expect("write starter scene");
    root
}

fn apply_payload(sessions: &mut EngineSessions, root: &Path, payload: &str) -> bool {
    let envelope: Value = serde_json::from_str(payload).expect("batch envelope");
    let actions = envelope["actions"].as_array().expect("actions");
    sessions
        .apply_batch(
            BatchRequest {
                game_dir: root,
                rel_path: LEVEL,
                label: envelope["label"].as_str().expect("label"),
                actions,
                actor: EngineActor::Agent,
                autosave: true,
                owner: None,
                base_revision: None,
            },
            resolve_batch_step,
        )
        .expect("batch invocation")
        .result
        .applied
}

#[test]
fn warehouse_key_door_repairs_and_verifies() {
    let root = temp_game();
    let mut sessions = EngineSessions::new();
    let before = sessions
        .open(&root, LEVEL)
        .expect("open scene")
        .entity_count;
    let transcript = [
        "I’ll inspect and build this as one reversible plan.\n<engine_query>{\"kind\":\"scene_summary\"}</engine_query>",
        "<engine_batch>{\"label\":\"Build warehouse key and locked door\",\"actions\":[{\"kind\":\"spawn\",\"template\":\"plane\",\"name\":\"WarehouseFloor\",\"at\":[0,0,0]},{\"kind\":\"spawn\",\"template\":\"locked-door\",\"name\":\"LockedDoor\",\"at\":[0,1,6]}]}</engine_batch>",
        "The template fault is located; I’ll repeat the build with a supported primitive.\n<engine_batch>{\"label\":\"Build warehouse key and locked door\",\"actions\":[{\"kind\":\"spawn\",\"template\":\"plane\",\"name\":\"WarehouseFloor\",\"at\":[0,0,0]},{\"kind\":\"spawn\",\"template\":\"cube\",\"name\":\"LockedDoor\",\"at\":[0,1,6]},{\"kind\":\"spawn\",\"template\":\"sphere\",\"name\":\"DoorKey\",\"at\":[2,0.5,0]},{\"kind\":\"set_tags\",\"entity\":\"LockedDoor\",\"tags\":[\"door\",\"locked\"]},{\"kind\":\"set_tags\",\"entity\":\"DoorKey\",\"tags\":[\"key\"]}]}</engine_batch>",
        "<engine_query>{\"kind\":\"playtest\",\"steps\":[{\"keys\":[\"KeyW\"],\"frames\":60,\"note\":\"approach door\"}]}</engine_query>",
        "<engine_query>{\"kind\":\"screenshot\",\"camera\":\"game\",\"annotate\":true}</engine_query>",
        "Verified: the repaired warehouse build passed the scripted playtest and final viewport capture.",
    ];
    assert!(transcript.len() <= ENGINE_AUTONOMY_MAX_ROUNDS);

    let mut scanner = EngineCallScanner::new();
    let mut visible = String::new();
    let mut rejected = 0;
    let mut applied = 0;
    let mut playtest_seen = false;
    let mut screenshot_seen = false;
    for round in transcript {
        // Split every round to prove arbitrary provider delta boundaries remain safe.
        let split = round.len() / 2;
        for chunk in [&round[..split], &round[split..]] {
            let (text, calls) = scanner.push(chunk);
            visible.push_str(&text);
            for call in calls {
                match call {
                    EngineCall::Batch(payload) => {
                        if apply_payload(&mut sessions, &root, &payload) {
                            applied += 1;
                        } else {
                            rejected += 1;
                        }
                    }
                    EngineCall::Query(payload) => {
                        let query: Value = serde_json::from_str(&payload).expect("query");
                        playtest_seen |= query["kind"] == "playtest";
                        screenshot_seen |= query["kind"] == "screenshot"
                            && query["camera"] == "game"
                            && query["annotate"] == true;
                    }
                    EngineCall::Action(_) => panic!("golden transcript uses labelled batches"),
                }
            }
        }
    }
    visible.push_str(&scanner.finish());

    assert_eq!(rejected, 1, "the loop must consume one real engine failure");
    assert_eq!(applied, 1, "the repair is one labelled transaction");
    assert!(playtest_seen && screenshot_seen);
    assert!(!visible.contains("engine_batch"));
    assert!(visible.contains("Verified:"));

    let state = sessions.open(&root, LEVEL).expect("reopen");
    assert_eq!(state.entity_count, before + 3);
    assert_eq!(
        state.undo_label.as_deref(),
        Some("Build warehouse key and locked door")
    );
    let undone = sessions.undo(&root, LEVEL).expect("one-click undo");
    assert_eq!(undone.entity_count, before);
    let _ignored = std::fs::remove_dir_all(root);
}
