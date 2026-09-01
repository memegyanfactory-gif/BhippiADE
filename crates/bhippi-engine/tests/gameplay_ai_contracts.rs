//! Phase 20 deterministic gameplay and gameplay-AI contract fixtures.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use bhippi_engine::gameplay_contract::*;
use bhippi_engine::navigation_ai::*;
use std::collections::BTreeMap;
use std::path::PathBuf;

fn fixture(name: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/engine/gameplay")
        .join(name);
    std::fs::read_to_string(path).expect("fixture committed")
}

fn gameplay() -> GameplayContractDocument {
    GameplayContractDocument::parse(&fixture("gameplay-v1.json")).expect("gameplay fixture")
}

fn ai() -> GameplayAiDocument {
    GameplayAiDocument::parse(&fixture("gameplay-ai-v1.json")).expect("AI fixture")
}

fn pool(current: u32, maximum: u32) -> ResourcePool {
    ResourcePool {
        current,
        maximum,
        regeneration_per_second: 10,
    }
}

#[test]
fn versioned_contract_fixtures_round_trip_and_future_major_blocks() {
    let gameplay = gameplay();
    assert_eq!(
        GameplayContractDocument::parse(
            &serde_json::to_string_pretty(&gameplay).expect("serialize")
        )
        .expect("reparse"),
        gameplay
    );
    let ai = ai();
    assert_eq!(
        GameplayAiDocument::parse(&serde_json::to_string_pretty(&ai).expect("serialize"))
            .expect("reparse"),
        ai
    );
    assert!(
        GameplayContractDocument::parse(&fixture("gameplay-v1.json").replacen(
            GAMEPLAY_CONTRACT_FORMAT,
            "bhippi-gameplay-contract@2",
            1
        ))
        .is_err()
    );
    assert!(
        GameplayAiDocument::parse(&fixture("gameplay-ai-v1.json").replacen(
            GAMEPLAY_AI_FORMAT,
            "bhippi-gameplay-ai@2",
            1
        ))
        .is_err()
    );
}

#[test]
fn damage_shield_death_and_hud_bindings_share_one_truth() {
    let mut combat = CombatResources {
        health: pool(30, 100),
        shield: pool(10, 50),
        stamina: pool(80, 100),
        mana: pool(20, 40),
        alive: true,
    };
    let outcome = combat
        .apply_damage(&DamagePacket {
            id: "hit-1".to_owned(),
            source: "enemy".to_owned(),
            target: "player".to_owned(),
            amount: 50,
            kind: DamageKind::Physical,
            bypass_shield: false,
        })
        .expect("damage");
    assert_eq!(outcome.absorbed_by_shield, 10);
    assert_eq!(outcome.health_damage, 30);
    assert!(outcome.killed);
    assert_eq!(combat.hud_bindings("player")["player.health"], 0);
    assert!(!combat.alive);
}

#[test]
fn inventory_pickup_equipment_and_snapshot_fail_closed() {
    let document = gameplay();
    let mut inventory = InventoryState {
        capacity_slots: 2,
        stacks: Vec::new(),
        equipment: BTreeMap::new(),
    };
    assert_eq!(
        inventory
            .add("pulse_rifle", 2, &document.items)
            .expect("add"),
        1
    );
    inventory
        .equip("pulse_rifle", &document.items)
        .expect("equip");
    inventory.validate(&document.items).expect("valid");

    let snapshot = InventorySnapshot {
        format: INVENTORY_SNAPSHOT_FORMAT.to_owned(),
        owner: "player".to_owned(),
        inventory,
    };
    let json = serde_json::to_string(&snapshot).expect("snapshot");
    assert_eq!(
        InventorySnapshot::parse(&json, &document.items).expect("parse"),
        snapshot
    );
    assert!(InventorySnapshot::parse(
        &json.replace(INVENTORY_SNAPSHOT_FORMAT, "bhippi-inventory@2"),
        &document.items
    )
    .is_err());
}

#[test]
fn door_checkpoint_and_pickup_interactions_are_atomic_and_typed() {
    let document = gameplay();
    let mut inventory = InventoryState {
        capacity_slots: 2,
        stacks: vec![ItemStack {
            item: "warehouse_key".to_owned(),
            amount: 1,
        }],
        equipment: BTreeMap::new(),
    };
    let mut door = document.interactions[0].clone();
    let outcome = door
        .activate("player", &mut inventory, &document.items)
        .expect("door unlocks");
    assert!(matches!(
        door.interaction,
        InteractionKind::Door {
            open: true,
            locked: false
        }
    ));
    assert!(matches!(outcome.event, GameplayEvent::Interacted { .. }));

    let mut checkpoint = document.interactions[1].clone();
    assert_eq!(
        checkpoint
            .activate("player", &mut inventory, &document.items)
            .expect("checkpoint")
            .respawn_mm,
        Some([0, 1000, 0])
    );
}

#[test]
fn objectives_complete_or_fail_only_from_declared_events() {
    let definition = gameplay().objectives[0].clone();
    let mut progress = ObjectiveProgress {
        definition,
        state: ObjectiveState::Active,
        observed_events: BTreeMap::new(),
    };
    assert!(progress.record_event("noise").expect("ignored").is_none());
    progress.record_event("key_collected").expect("key");
    assert_eq!(
        match_state(std::slice::from_ref(&progress)),
        MatchState::Running
    );
    progress.record_event("exit_opened").expect("exit");
    assert_eq!(progress.state, ObjectiveState::Completed);
    assert_eq!(match_state(&[progress]), MatchState::Won);

    let mut failed = ObjectiveProgress {
        definition: gameplay().objectives[0].clone(),
        state: ObjectiveState::Active,
        observed_events: BTreeMap::new(),
    };
    failed.record_event("player_died").expect("failure");
    assert_eq!(match_state(&[failed]), MatchState::Lost);
}

#[test]
fn weapon_fire_spread_falloff_ammo_and_reload_are_deterministic() {
    let mut weapon = gameplay().weapons[0].clone();
    weapon.attachments.push(WeaponAttachment {
        id: "heavy_barrel".to_owned(),
        damage_modifier_bps: 2500,
        spread_modifier_bps: -1000,
        recoil_modifier_bps: 1000,
    });
    let mut first = WeaponState {
        loaded: 2,
        reserve: 10,
        reload_complete_tick: None,
        shot_sequence: 0,
    };
    let mut second = first.clone();
    let shot = weapon.fire(&mut first, 0, 20_000).expect("fires");
    assert_eq!(
        shot,
        weapon.fire(&mut second, 0, 20_000).expect("same shot")
    );
    assert!(shot.damage > 0);
    assert!(shot.recoil_millidegrees > weapon.recoil_millidegrees);
    assert_eq!(first.loaded, 1);
    weapon.start_reload(&mut first, 10).expect("reload starts");
    assert!(weapon.finish_reload(&mut first, 50).is_err());
    assert_eq!(
        weapon.finish_reload(&mut first, 100).expect("reload done"),
        5
    );
}

#[test]
fn unconfigured_navigation_is_honest_and_path_results_are_bounded() {
    let document = ai();
    let query = PathQuery {
        request_id: "path-1".to_owned(),
        start: NavPointMm([0, 0, 0]),
        goal: NavPointMm([1000, 0, 0]),
        allowed_areas: vec!["walkable".to_owned()],
        blocked_obstacle_ids: Vec::new(),
        allow_partial: false,
    };
    let unsupported = document
        .navigation
        .unsupported_result(&query)
        .expect("explicit unsupported");
    assert_eq!(unsupported.status, PathStatus::UnsupportedBackend);
    unsupported
        .validate(&query, document.navigation.limits)
        .expect("valid failure");

    let overflow = PathResult {
        request_id: query.request_id.clone(),
        status: PathStatus::Complete,
        waypoints: vec![query.start, query.goal],
        total_cost_milli: document.navigation.limits.max_total_cost_milli + 1,
        visited_nodes: 2,
        reason: String::new(),
    };
    assert!(overflow
        .validate(&query, document.navigation.limits)
        .is_err());
}

#[test]
fn blackboard_state_machine_behavior_and_utility_are_deterministic() {
    let document = ai();
    let mut blackboard = Blackboard::default();
    blackboard
        .set(
            &document.blackboard_schema,
            "target_visible",
            BlackboardValue::Boolean(true),
        )
        .expect("visible");
    blackboard
        .set(
            &document.blackboard_schema,
            "distance_mm",
            BlackboardValue::Integer(15_000),
        )
        .expect("distance");
    blackboard
        .validate(&document.blackboard_schema)
        .expect("typed");
    assert_eq!(
        document
            .state_machine
            .step("idle", &blackboard)
            .expect("transition"),
        "chase"
    );
    assert_eq!(
        document
            .behavior_tree
            .decide(&blackboard)
            .expect("behavior")
            .action,
        Some(AiAction::Chase)
    );
    let utility = decide_utility(&document.utility_options, &blackboard).expect("utility");
    assert_eq!(utility.action, AiAction::Flee);
    assert_eq!(utility.score_bps, 5_000);

    let mut wrong = blackboard;
    assert!(wrong
        .set(
            &document.blackboard_schema,
            "distance_mm",
            BlackboardValue::Text("near".to_owned())
        )
        .is_err());
}

#[test]
fn perception_is_bounded_sorted_and_expires() {
    let limits = ai().perception_limits;
    let mut memory = PerceptionMemory::default();
    for (id, strength) in [("weak", 1000), ("strong", 9000)] {
        memory
            .observe(
                PerceptionObservation {
                    id: id.to_owned(),
                    kind: PerceptionKind::Sight,
                    subject: "player".to_owned(),
                    position: NavPointMm([0, 0, 0]),
                    strength_bps: strength,
                    observed_tick: 10,
                    expires_tick: 20,
                },
                10,
                limits,
            )
            .expect("observe");
    }
    assert_eq!(memory.observations[0].id, "strong");
    memory.expire(20);
    assert!(memory.observations.is_empty());
}

#[test]
fn behavior_cycles_and_budget_overruns_block() {
    let mut document = ai();
    let root = document.behavior_tree.root.clone();
    if let BehaviorNodeKind::Selector { children } = &mut document.behavior_tree.nodes[0].node {
        children[0] = root;
    }
    assert!(document.behavior_tree.validate().is_err());

    let mut document = ai();
    document.behavior_tree.limits.max_nodes = 1;
    assert!(document.behavior_tree.validate().is_err());
}
