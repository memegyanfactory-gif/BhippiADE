#![allow(clippy::expect_used)]

use bhippi_engine::runtime_save::{
    PersistedEntity, PersistedValue, RuntimeSave, RuntimeSaveError, RuntimeSaveLimits,
    RUNTIME_SAVE_FORMAT,
};
use std::collections::BTreeMap;

fn fixture() -> RuntimeSave {
    RuntimeSave {
        format: RUNTIME_SAVE_FORMAT.to_owned(),
        game_id: "warehouse".to_owned(),
        build_id: "build-7".to_owned(),
        save_id: "slot-1".to_owned(),
        tick: 120,
        seed: 42,
        active_level: "Main".to_owned(),
        entities: vec![PersistedEntity {
            stable_id: "player".to_owned(),
            source_scene: "scenes/main.bscn.json".to_owned(),
            source_prefab: None,
            position: [1.0, 2.0, 3.0],
            rotation: [0.0, 0.0, 0.0],
            scale: [1.0, 1.0, 1.0],
            state: BTreeMap::from([("health".to_owned(), PersistedValue::Integer(90))]),
        }],
        globals: BTreeMap::from([("door_open".to_owned(), PersistedValue::Bool(true))]),
        checkpoint_hash: String::new(),
        parent_checkpoint_hash: None,
    }
}

#[test]
fn sealed_save_round_trips_and_validates() {
    let save = fixture().seal().expect("fixture seals");
    save.validate(&RuntimeSaveLimits::default())
        .expect("fixture valid");
    let encoded = serde_json::to_vec(&save).expect("fixture encodes");
    let decoded: RuntimeSave = serde_json::from_slice(&encoded).expect("fixture decodes");
    assert_eq!(decoded, save);
}

#[test]
fn canonical_hash_does_not_depend_on_entity_input_order() {
    let mut first = fixture();
    let mut second = fixture();
    let mut extra = first.entities[0].clone();
    extra.stable_id = "key".to_owned();
    first.entities.push(extra.clone());
    second.entities.insert(0, extra);
    assert_eq!(
        first.canonical_checkpoint_hash().expect("first hashes"),
        second.canonical_checkpoint_hash().expect("second hashes")
    );
}

#[test]
fn duplicate_ids_non_finite_values_and_tampering_fail_closed() {
    let limits = RuntimeSaveLimits::default();
    let mut duplicate = fixture();
    duplicate.entities.push(duplicate.entities[0].clone());
    duplicate = duplicate
        .seal()
        .expect("duplicate can be sealed before validation");
    assert!(matches!(
        duplicate.validate(&limits),
        Err(RuntimeSaveError::DuplicateEntity(_))
    ));

    let mut non_finite = fixture();
    non_finite.entities[0].position[0] = f32::NAN;
    non_finite.checkpoint_hash = "untrusted".to_owned();
    assert!(matches!(
        non_finite.validate(&limits),
        Err(RuntimeSaveError::NonFinite(_))
    ));

    let mut tampered = fixture().seal().expect("fixture seals");
    tampered.tick += 1;
    assert_eq!(
        tampered.validate(&limits),
        Err(RuntimeSaveError::HashMismatch)
    );
}

#[test]
fn limits_block_recursive_or_oversized_state() {
    let mut save = fixture();
    save.globals
        .insert("large".to_owned(), PersistedValue::Text("x".repeat(32)));
    save = save.seal().expect("fixture seals");
    let limits = RuntimeSaveLimits {
        text_bytes: 4,
        ..RuntimeSaveLimits::default()
    };
    assert!(matches!(
        save.validate(&limits),
        Err(RuntimeSaveError::Limit {
            resource: "text_bytes",
            ..
        })
    ));
}
