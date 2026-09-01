#![allow(clippy::expect_used)]

use bhippi_engine::runtime_save::{
    PersistedEntity, PersistedValue, RuntimeSave, RuntimeSaveLimits, RUNTIME_SAVE_FORMAT,
};
use bhippi_engine::runtime_save_store::{
    resolve_save_sync, OpaqueRemoteSave, OpaqueSavePayload, RuntimeSaveMigrationRegistry,
    RuntimeSaveStore, RuntimeSaveStoreError, SaveLoadSource, SaveSyncResolution,
    SaveWriteDisposition,
};
use bhippi_types::AssetId;
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

const LEGACY_V0: &str = include_str!("../../../tests/fixtures/engine/runtime_save/legacy-v0.json");

fn temp_root(label: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("bhippi-save-{label}-{}", AssetId::new()));
    fs::create_dir_all(&root).expect("create isolated save root");
    root
}

fn fixture(save_id: &str, tick: u64) -> RuntimeSave {
    RuntimeSave {
        format: RUNTIME_SAVE_FORMAT.to_owned(),
        game_id: "warehouse".to_owned(),
        build_id: "build-7".to_owned(),
        save_id: save_id.to_owned(),
        tick,
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
    .seal()
    .expect("seal fixture")
}

fn child(parent: &RuntimeSave, tick: u64, health: i64) -> RuntimeSave {
    let mut next = parent.clone();
    next.tick = tick;
    next.parent_checkpoint_hash = Some(parent.checkpoint_hash.clone());
    next.checkpoint_hash.clear();
    next.entities[0]
        .state
        .insert("health".to_owned(), PersistedValue::Integer(health));
    next.seal().expect("seal child")
}

#[test]
fn atomic_write_keeps_valid_backup_and_explicit_rollback() {
    let root = temp_root("atomic");
    let store = RuntimeSaveStore::new(&root, RuntimeSaveLimits::default());
    let first = fixture("slot-1", 100);
    let created = store.write(&first).expect("create save");
    assert_eq!(created.disposition, SaveWriteDisposition::Created);

    let second = child(&first, 101, 80);
    let replaced = store.write(&second).expect("replace save");
    assert_eq!(replaced.disposition, SaveWriteDisposition::Replaced);
    assert_eq!(
        replaced.previous_checkpoint_hash,
        Some(first.checkpoint_hash.clone())
    );
    assert!(root.join("slot-1.save.json.bak").is_file());
    assert_eq!(store.load("slot-1").expect("load current").save, second);

    let rollback = store
        .rollback_to_backup("slot-1")
        .expect("rollback to valid backup");
    assert_eq!(rollback.disposition, SaveWriteDisposition::RolledBack);
    assert_eq!(store.load("slot-1").expect("load rollback").save, first);
    assert!(root.join("slot-1.save.json.pre-rollback").is_file());
    fs::remove_dir_all(root).expect("remove isolated root");
}

#[test]
fn corrupt_primary_recovers_valid_backup_and_quarantines_bad_bytes() {
    let root = temp_root("recovery");
    let store = RuntimeSaveStore::new(&root, RuntimeSaveLimits::default());
    let first = fixture("slot-1", 100);
    store.write(&first).expect("first");
    let second = child(&first, 101, 80);
    store.write(&second).expect("second creates backup");
    fs::write(root.join("slot-1.save.json"), b"not-json").expect("corrupt primary");

    let recovered = store.load("slot-1").expect("recover backup");
    assert_eq!(recovered.source, SaveLoadSource::RecoveredBackup);
    assert_eq!(recovered.save, first);
    assert!(root.join("slot-1.save.json.corrupt").is_file());
    assert_eq!(
        store.load("slot-1").expect("repaired primary").source,
        SaveLoadSource::Primary
    );
    fs::remove_dir_all(root).expect("remove isolated root");
}

#[test]
fn corrupt_primary_and_backup_fail_without_inventing_state() {
    let root = temp_root("no-recovery");
    fs::write(root.join("slot-1.save.json"), b"bad primary").expect("primary");
    fs::write(root.join("slot-1.save.json.bak"), b"bad backup").expect("backup");
    let store = RuntimeSaveStore::new(&root, RuntimeSaveLimits::default());
    let error = store.load("slot-1").expect_err("nothing valid");
    assert!(matches!(
        error,
        RuntimeSaveStoreError::NoRecoverableSave { .. }
    ));
    fs::remove_dir_all(root).expect("remove isolated root");
}

#[test]
fn explicit_v0_migration_rewrites_current_and_retains_legacy_backup() {
    let root = temp_root("migration");
    fs::write(root.join("slot-legacy.save.json"), LEGACY_V0).expect("legacy primary");
    let store = RuntimeSaveStore::new(&root, RuntimeSaveLimits::default());
    let loaded = store.load("slot-legacy").expect("migrate known v0");
    assert_eq!(loaded.source, SaveLoadSource::MigratedPrimary);
    assert_eq!(loaded.save.format, RUNTIME_SAVE_FORMAT);
    assert_eq!(loaded.save.active_level, "Main");
    assert!(loaded.migration.is_some());
    assert!(fs::read_to_string(root.join("slot-legacy.save.json"))
        .expect("current bytes")
        .contains("bhippi-runtime-save@1"));
    assert!(fs::read_to_string(root.join("slot-legacy.save.json.bak"))
        .expect("legacy backup")
        .contains("bhippi-runtime-save@0"));

    let no_migrations = RuntimeSaveStore::new(&root, RuntimeSaveLimits::default())
        .with_migrations(RuntimeSaveMigrationRegistry::new(Vec::new()).expect("empty registry"));
    fs::write(root.join("slot-legacy.save.json"), LEGACY_V0).expect("restore legacy");
    fs::remove_file(root.join("slot-legacy.save.json.bak")).expect("remove fallback");
    let error = no_migrations
        .load("slot-legacy")
        .expect_err("migration must be registered");
    assert!(matches!(
        error,
        RuntimeSaveStoreError::NoRecoverableSave { .. }
    ));
    fs::remove_dir_all(root).expect("remove isolated root");
}

#[test]
fn writes_are_chain_guarded_and_slot_names_cannot_escape() {
    let root = temp_root("conflict");
    let store = RuntimeSaveStore::new(&root, RuntimeSaveLimits::default());
    let first = fixture("slot-1", 100);
    store.write(&first).expect("first");
    let disconnected = fixture("slot-1", 200);
    let error = store.write(&disconnected).expect_err("stale writer");
    assert!(matches!(
        error,
        RuntimeSaveStoreError::CheckpointConflict { .. }
    ));
    assert!(matches!(
        store.load("../escape"),
        Err(RuntimeSaveStoreError::UnsafeSaveId(_))
    ));
    fs::remove_dir_all(root).expect("remove isolated root");
}

#[test]
fn opaque_sync_contract_detects_direction_and_divergent_siblings() {
    let limits = RuntimeSaveLimits::default();
    let base = fixture("slot-1", 100);
    let local = child(&base, 101, 80);
    let remote_base = OpaqueRemoteSave {
        revision: "remote-r1".to_owned(),
        payload: OpaqueSavePayload::from_save(&base, &limits).expect("base payload"),
    };
    assert_eq!(
        resolve_save_sync(&local, &remote_base, &limits).expect("upload direction"),
        SaveSyncResolution::UploadLocal {
            expected_remote_revision: "remote-r1".to_owned()
        }
    );

    let remote_child = child(&local, 102, 70);
    let remote = OpaqueRemoteSave {
        revision: "remote-r2".to_owned(),
        payload: OpaqueSavePayload::from_save(&remote_child, &limits).expect("child payload"),
    };
    assert_eq!(
        resolve_save_sync(&local, &remote, &limits).expect("download direction"),
        SaveSyncResolution::DownloadRemote {
            remote_revision: "remote-r2".to_owned()
        }
    );

    let sibling = child(&base, 101, 50);
    let divergent = OpaqueRemoteSave {
        revision: "remote-r3".to_owned(),
        payload: OpaqueSavePayload::from_save(&sibling, &limits).expect("sibling payload"),
    };
    assert!(matches!(
        resolve_save_sync(&local, &divergent, &limits).expect("explicit conflict"),
        SaveSyncResolution::Conflict {
            common_parent_hash: Some(_),
            ..
        }
    ));

    let mut tampered = divergent;
    tampered.payload.bytes.push(0);
    assert!(matches!(
        resolve_save_sync(&local, &tampered, &limits),
        Err(RuntimeSaveStoreError::Sync(_))
    ));
}
