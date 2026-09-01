//! Atomic local runtime-save persistence, migration and provider-neutral sync contracts.
//!
//! Providers receive validated opaque bytes and revision strings only. This module never stores
//! credentials, provider SDK objects or secret-bearing configuration.

use crate::runtime_save::{
    PersistedEntity, PersistedValue, RuntimeSave, RuntimeSaveError, RuntimeSaveLimits,
    RUNTIME_SAVE_FORMAT,
};
use bhippi_types::AssetId;
use serde::{Deserialize, Serialize};
use specta::Type;
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

pub const LEGACY_RUNTIME_SAVE_V0_FORMAT: &str = "bhippi-runtime-save@0";
pub const OPAQUE_SAVE_PAYLOAD_FORMAT: &str = "bhippi-opaque-save-payload@1";

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeSaveMigrationId {
    V0ToV1,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct RuntimeSaveMigrationRecord {
    pub migration: RuntimeSaveMigrationId,
    pub from_format: String,
    pub to_format: String,
    pub source_hash: String,
    pub checkpoint_hash: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct RuntimeSaveMigrationRegistry {
    pub migrations: Vec<RuntimeSaveMigrationId>,
}

impl Default for RuntimeSaveMigrationRegistry {
    fn default() -> Self {
        Self {
            migrations: vec![RuntimeSaveMigrationId::V0ToV1],
        }
    }
}

impl RuntimeSaveMigrationRegistry {
    pub fn new(migrations: Vec<RuntimeSaveMigrationId>) -> Result<Self, RuntimeSaveStoreError> {
        let unique = migrations.iter().copied().collect::<BTreeSet<_>>();
        if unique.len() != migrations.len() {
            return Err(RuntimeSaveStoreError::MigrationRegistry(
                "migration ids must be unique".to_owned(),
            ));
        }
        Ok(Self { migrations })
    }

    pub fn decode(
        &self,
        bytes: &[u8],
        limits: &RuntimeSaveLimits,
    ) -> Result<DecodedRuntimeSave, RuntimeSaveStoreError> {
        if bytes.len() > limits.encoded_bytes {
            return Err(RuntimeSaveStoreError::Limit {
                actual: bytes.len(),
                limit: limits.encoded_bytes,
            });
        }
        let value: serde_json::Value = serde_json::from_slice(bytes)
            .map_err(|error| RuntimeSaveStoreError::Decode(error.to_string()))?;
        let format = value
            .get("format")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| RuntimeSaveStoreError::Decode("save has no string format".to_owned()))?;
        match format {
            RUNTIME_SAVE_FORMAT => {
                let save: RuntimeSave = serde_json::from_value(value)
                    .map_err(|error| RuntimeSaveStoreError::Decode(error.to_string()))?;
                save.validate(limits)?;
                Ok(DecodedRuntimeSave {
                    save,
                    migration: None,
                })
            }
            LEGACY_RUNTIME_SAVE_V0_FORMAT => {
                if !self.migrations.contains(&RuntimeSaveMigrationId::V0ToV1) {
                    return Err(RuntimeSaveStoreError::MigrationUnavailable(
                        format.to_owned(),
                    ));
                }
                let legacy: RuntimeSaveV0 = serde_json::from_value(value)
                    .map_err(|error| RuntimeSaveStoreError::Decode(error.to_string()))?;
                let source_hash = blake3::hash(bytes).to_hex().to_string();
                let save = legacy.migrate()?.seal()?;
                save.validate(limits)?;
                let checkpoint_hash = save.checkpoint_hash.clone();
                Ok(DecodedRuntimeSave {
                    save,
                    migration: Some(RuntimeSaveMigrationRecord {
                        migration: RuntimeSaveMigrationId::V0ToV1,
                        from_format: LEGACY_RUNTIME_SAVE_V0_FORMAT.to_owned(),
                        to_format: RUNTIME_SAVE_FORMAT.to_owned(),
                        source_hash,
                        checkpoint_hash,
                    }),
                })
            }
            unsupported => Err(RuntimeSaveStoreError::MigrationUnavailable(
                unsupported.to_owned(),
            )),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct DecodedRuntimeSave {
    pub save: RuntimeSave,
    pub migration: Option<RuntimeSaveMigrationRecord>,
}

#[derive(Clone, Debug, Deserialize)]
struct RuntimeSaveV0 {
    format: String,
    game_id: String,
    build_id: String,
    save_id: String,
    tick: u64,
    seed: u64,
    level: String,
    #[serde(default)]
    entities: Vec<PersistedEntity>,
    #[serde(default)]
    globals: BTreeMap<String, PersistedValue>,
}

impl RuntimeSaveV0 {
    fn migrate(self) -> Result<RuntimeSave, RuntimeSaveStoreError> {
        if self.format != LEGACY_RUNTIME_SAVE_V0_FORMAT {
            return Err(RuntimeSaveStoreError::MigrationUnavailable(self.format));
        }
        Ok(RuntimeSave {
            format: RUNTIME_SAVE_FORMAT.to_owned(),
            game_id: self.game_id,
            build_id: self.build_id,
            save_id: self.save_id,
            tick: self.tick,
            seed: self.seed,
            active_level: self.level,
            entities: self.entities,
            globals: self.globals,
            checkpoint_hash: String::new(),
            parent_checkpoint_hash: None,
        })
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum SaveWriteDisposition {
    Created,
    Replaced,
    Unchanged,
    Migrated,
    RecoveredBackup,
    RolledBack,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct SaveWriteReceipt {
    pub save_id: String,
    pub checkpoint_hash: String,
    pub previous_checkpoint_hash: Option<String>,
    pub encoded_bytes: usize,
    pub disposition: SaveWriteDisposition,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum SaveLoadSource {
    Primary,
    MigratedPrimary,
    RecoveredBackup,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SaveLoadOutcome {
    pub save: RuntimeSave,
    pub source: SaveLoadSource,
    pub migration: Option<RuntimeSaveMigrationRecord>,
}

#[derive(Clone, Debug)]
pub struct RuntimeSaveStore {
    root: PathBuf,
    limits: RuntimeSaveLimits,
    migrations: RuntimeSaveMigrationRegistry,
}

impl RuntimeSaveStore {
    #[must_use]
    pub fn new(root: impl Into<PathBuf>, limits: RuntimeSaveLimits) -> Self {
        Self {
            root: root.into(),
            limits,
            migrations: RuntimeSaveMigrationRegistry::default(),
        }
    }

    #[must_use]
    pub fn with_migrations(mut self, migrations: RuntimeSaveMigrationRegistry) -> Self {
        self.migrations = migrations;
        self
    }

    pub fn write(&self, save: &RuntimeSave) -> Result<SaveWriteReceipt, RuntimeSaveStoreError> {
        validate_save_id(&save.save_id)?;
        save.validate(&self.limits)?;
        std::fs::create_dir_all(&self.root)
            .map_err(|error| io_error("create save directory", &self.root, error))?;
        let paths = self.slot_paths(&save.save_id)?;
        let bytes = encode_current(save, &self.limits)?;
        if !paths.primary.exists() {
            if save.parent_checkpoint_hash.is_some() {
                return Err(RuntimeSaveStoreError::CheckpointConflict {
                    expected_parent: None,
                    actual_parent: save.parent_checkpoint_hash.clone(),
                });
            }
            install_bytes(&paths.primary, &bytes, None)?;
            return Ok(receipt(
                save,
                None,
                bytes.len(),
                SaveWriteDisposition::Created,
            ));
        }

        let current = self
            .read_candidate(&paths.primary)
            .map_err(|error| RuntimeSaveStoreError::ExistingPrimaryInvalid(error.to_string()))?;
        if current.save.checkpoint_hash == save.checkpoint_hash {
            return Ok(receipt(
                save,
                Some(current.save.checkpoint_hash),
                bytes.len(),
                SaveWriteDisposition::Unchanged,
            ));
        }
        let expected = Some(current.save.checkpoint_hash.clone());
        if save.parent_checkpoint_hash != expected {
            return Err(RuntimeSaveStoreError::CheckpointConflict {
                expected_parent: expected,
                actual_parent: save.parent_checkpoint_hash.clone(),
            });
        }
        install_bytes(&paths.primary, &bytes, Some(&paths.backup))?;
        Ok(receipt(
            save,
            Some(current.save.checkpoint_hash),
            bytes.len(),
            SaveWriteDisposition::Replaced,
        ))
    }

    /// Load and validate a slot. A bad/missing primary is restored from a validated backup.
    /// A migrated primary is atomically rewritten in the current format with the legacy bytes
    /// retained as its backup.
    pub fn load(&self, save_id: &str) -> Result<SaveLoadOutcome, RuntimeSaveStoreError> {
        let paths = self.slot_paths(save_id)?;
        match self.read_candidate(&paths.primary) {
            Ok(decoded) => {
                if let Some(migration) = decoded.migration.clone() {
                    let bytes = encode_current(&decoded.save, &self.limits)?;
                    install_bytes(&paths.primary, &bytes, Some(&paths.backup))?;
                    Ok(SaveLoadOutcome {
                        save: decoded.save,
                        source: SaveLoadSource::MigratedPrimary,
                        migration: Some(migration),
                    })
                } else {
                    Ok(SaveLoadOutcome {
                        save: decoded.save,
                        source: SaveLoadSource::Primary,
                        migration: None,
                    })
                }
            }
            Err(primary_error) => match self.read_candidate(&paths.backup) {
                Ok(decoded) => {
                    let bytes = encode_current(&decoded.save, &self.limits)?;
                    install_bytes(&paths.primary, &bytes, Some(&paths.corrupt))?;
                    Ok(SaveLoadOutcome {
                        save: decoded.save,
                        source: SaveLoadSource::RecoveredBackup,
                        migration: decoded.migration,
                    })
                }
                Err(backup_error) => Err(RuntimeSaveStoreError::NoRecoverableSave {
                    primary: primary_error.to_string(),
                    backup: backup_error.to_string(),
                }),
            },
        }
    }

    pub fn rollback_to_backup(
        &self,
        save_id: &str,
    ) -> Result<SaveWriteReceipt, RuntimeSaveStoreError> {
        let paths = self.slot_paths(save_id)?;
        let backup = self.read_candidate(&paths.backup)?;
        let current_hash = self
            .read_candidate(&paths.primary)
            .ok()
            .map(|decoded| decoded.save.checkpoint_hash);
        let bytes = encode_current(&backup.save, &self.limits)?;
        install_bytes(&paths.primary, &bytes, Some(&paths.pre_rollback))?;
        Ok(receipt(
            &backup.save,
            current_hash,
            bytes.len(),
            SaveWriteDisposition::RolledBack,
        ))
    }

    fn slot_paths(&self, save_id: &str) -> Result<SaveSlotPaths, RuntimeSaveStoreError> {
        validate_save_id(save_id)?;
        let base = format!("{save_id}.save.json");
        Ok(SaveSlotPaths {
            primary: self.root.join(&base),
            backup: self.root.join(format!("{base}.bak")),
            corrupt: self.root.join(format!("{base}.corrupt")),
            pre_rollback: self.root.join(format!("{base}.pre-rollback")),
        })
    }

    fn read_candidate(&self, path: &Path) -> Result<DecodedRuntimeSave, RuntimeSaveStoreError> {
        let metadata =
            std::fs::metadata(path).map_err(|error| io_error("inspect save", path, error))?;
        let declared = usize::try_from(metadata.len()).unwrap_or(usize::MAX);
        if declared > self.limits.encoded_bytes {
            return Err(RuntimeSaveStoreError::Limit {
                actual: declared,
                limit: self.limits.encoded_bytes,
            });
        }
        let file = File::open(path).map_err(|error| io_error("open save", path, error))?;
        let cap = u64::try_from(self.limits.encoded_bytes)
            .unwrap_or(u64::MAX)
            .saturating_add(1);
        let mut bytes = Vec::with_capacity(declared);
        file.take(cap)
            .read_to_end(&mut bytes)
            .map_err(|error| io_error("read save", path, error))?;
        self.migrations.decode(&bytes, &self.limits)
    }
}

struct SaveSlotPaths {
    primary: PathBuf,
    backup: PathBuf,
    corrupt: PathBuf,
    pre_rollback: PathBuf,
}

fn install_bytes(
    primary: &Path,
    bytes: &[u8],
    previous_destination: Option<&Path>,
) -> Result<(), RuntimeSaveStoreError> {
    let parent = primary.parent().ok_or_else(|| RuntimeSaveStoreError::Io {
        operation: "resolve save parent",
        path: primary.display().to_string(),
        reason: "save path has no parent".to_owned(),
    })?;
    std::fs::create_dir_all(parent)
        .map_err(|error| io_error("create save directory", parent, error))?;
    let file_name = primary
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("runtime-save");
    let temp = parent.join(format!(".{file_name}.{}.tmp", AssetId::new()));
    write_synced(&temp, bytes)?;

    if primary.exists() {
        if let Some(destination) = previous_destination {
            if destination.exists() {
                std::fs::remove_file(destination).map_err(|error| {
                    io_error("remove previous recovery file", destination, error)
                })?;
            }
            if let Err(error) = std::fs::rename(primary, destination) {
                let _ = std::fs::remove_file(&temp);
                return Err(io_error("preserve previous save", primary, error));
            }
        } else {
            let _ = std::fs::remove_file(&temp);
            return Err(RuntimeSaveStoreError::Io {
                operation: "install save",
                path: primary.display().to_string(),
                reason: "destination exists without a recovery path".to_owned(),
            });
        }
    }
    if let Err(error) = std::fs::rename(&temp, primary) {
        if let Some(destination) = previous_destination {
            if destination.exists() && !primary.exists() {
                let _ = std::fs::copy(destination, primary);
            }
        }
        let _ = std::fs::remove_file(&temp);
        return Err(io_error("install save", primary, error));
    }
    Ok(())
}

fn write_synced(path: &Path, bytes: &[u8]) -> Result<(), RuntimeSaveStoreError> {
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(path)
        .map_err(|error| io_error("create temporary save", path, error))?;
    file.write_all(bytes)
        .map_err(|error| io_error("write temporary save", path, error))?;
    file.sync_all()
        .map_err(|error| io_error("sync temporary save", path, error))?;
    Ok(())
}

fn encode_current(
    save: &RuntimeSave,
    limits: &RuntimeSaveLimits,
) -> Result<Vec<u8>, RuntimeSaveStoreError> {
    save.validate(limits)?;
    let bytes = serde_json::to_vec_pretty(save)
        .map_err(|error| RuntimeSaveStoreError::Decode(error.to_string()))?;
    if bytes.len() > limits.encoded_bytes {
        return Err(RuntimeSaveStoreError::Limit {
            actual: bytes.len(),
            limit: limits.encoded_bytes,
        });
    }
    Ok(bytes)
}

fn receipt(
    save: &RuntimeSave,
    previous_checkpoint_hash: Option<String>,
    encoded_bytes: usize,
    disposition: SaveWriteDisposition,
) -> SaveWriteReceipt {
    SaveWriteReceipt {
        save_id: save.save_id.clone(),
        checkpoint_hash: save.checkpoint_hash.clone(),
        previous_checkpoint_hash,
        encoded_bytes,
        disposition,
    }
}

fn validate_save_id(save_id: &str) -> Result<(), RuntimeSaveStoreError> {
    if save_id.is_empty()
        || save_id.len() > 64
        || !save_id
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
    {
        return Err(RuntimeSaveStoreError::UnsafeSaveId(save_id.to_owned()));
    }
    Ok(())
}

fn io_error(operation: &'static str, path: &Path, error: std::io::Error) -> RuntimeSaveStoreError {
    RuntimeSaveStoreError::Io {
        operation,
        path: path.display().to_string(),
        reason: error.to_string(),
    }
}

#[derive(Clone, Debug, thiserror::Error, Eq, PartialEq)]
pub enum RuntimeSaveStoreError {
    #[error(transparent)]
    Validation(#[from] RuntimeSaveError),
    #[error("runtime-save bytes could not be decoded: {0}")]
    Decode(String),
    #[error("runtime-save payload exceeds encoded byte limit ({actual} > {limit})")]
    Limit { actual: usize, limit: usize },
    #[error("save id `{0}` is unsafe")]
    UnsafeSaveId(String),
    #[error("no registered migration accepts `{0}`")]
    MigrationUnavailable(String),
    #[error("invalid migration registry: {0}")]
    MigrationRegistry(String),
    #[error("existing primary save is invalid: {0}")]
    ExistingPrimaryInvalid(String),
    #[error("checkpoint parent conflict: expected {expected_parent:?}, got {actual_parent:?}")]
    CheckpointConflict {
        expected_parent: Option<String>,
        actual_parent: Option<String>,
    },
    #[error("no recoverable save; primary: {primary}; backup: {backup}")]
    NoRecoverableSave { primary: String, backup: String },
    #[error("{operation} failed for {path}: {reason}")]
    Io {
        operation: &'static str,
        path: String,
        reason: String,
    },
    #[error("opaque sync contract is invalid: {0}")]
    Sync(String),
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct OpaqueSavePayload {
    pub format: String,
    pub save_id: String,
    pub checkpoint_hash: String,
    pub parent_checkpoint_hash: Option<String>,
    pub payload_hash: String,
    /// Encoded save bytes. A provider must treat these as opaque application data.
    pub bytes: Vec<u8>,
}

impl OpaqueSavePayload {
    pub fn from_save(
        save: &RuntimeSave,
        limits: &RuntimeSaveLimits,
    ) -> Result<Self, RuntimeSaveStoreError> {
        let bytes = encode_current(save, limits)?;
        let payload_hash = blake3::hash(&bytes).to_hex().to_string();
        Ok(Self {
            format: OPAQUE_SAVE_PAYLOAD_FORMAT.to_owned(),
            save_id: save.save_id.clone(),
            checkpoint_hash: save.checkpoint_hash.clone(),
            parent_checkpoint_hash: save.parent_checkpoint_hash.clone(),
            payload_hash,
            bytes,
        })
    }

    pub fn validate(&self, limits: &RuntimeSaveLimits) -> Result<(), RuntimeSaveStoreError> {
        if self.format != OPAQUE_SAVE_PAYLOAD_FORMAT {
            return Err(RuntimeSaveStoreError::Sync(format!(
                "unsupported payload format {:?}",
                self.format
            )));
        }
        validate_save_id(&self.save_id)?;
        if self.bytes.len() > limits.encoded_bytes
            || self.checkpoint_hash.is_empty()
            || self.payload_hash != blake3::hash(&self.bytes).to_hex().to_string()
        {
            return Err(RuntimeSaveStoreError::Sync(
                "payload is oversized, unsealed or hash-mismatched".to_owned(),
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct OpaqueRemoteSave {
    /// Provider-owned concurrency token. It is opaque and never interpreted as credentials.
    pub revision: String,
    pub payload: OpaqueSavePayload,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
#[serde(tag = "resolution", rename_all = "snake_case")]
pub enum SaveSyncResolution {
    InSync,
    UploadLocal {
        expected_remote_revision: String,
    },
    DownloadRemote {
        remote_revision: String,
    },
    Conflict {
        local_checkpoint_hash: String,
        remote_checkpoint_hash: String,
        common_parent_hash: Option<String>,
    },
}

pub fn resolve_save_sync(
    local: &RuntimeSave,
    remote: &OpaqueRemoteSave,
    limits: &RuntimeSaveLimits,
) -> Result<SaveSyncResolution, RuntimeSaveStoreError> {
    local.validate(limits)?;
    remote.payload.validate(limits)?;
    if remote.revision.trim().is_empty() || remote.payload.save_id != local.save_id {
        return Err(RuntimeSaveStoreError::Sync(
            "remote revision is empty or belongs to another save slot".to_owned(),
        ));
    }
    if remote.payload.checkpoint_hash == local.checkpoint_hash {
        return Ok(SaveSyncResolution::InSync);
    }
    if remote.payload.parent_checkpoint_hash.as_deref() == Some(local.checkpoint_hash.as_str()) {
        return Ok(SaveSyncResolution::DownloadRemote {
            remote_revision: remote.revision.clone(),
        });
    }
    if local.parent_checkpoint_hash.as_deref() == Some(remote.payload.checkpoint_hash.as_str()) {
        return Ok(SaveSyncResolution::UploadLocal {
            expected_remote_revision: remote.revision.clone(),
        });
    }
    let common_parent_hash = match (
        local.parent_checkpoint_hash.as_deref(),
        remote.payload.parent_checkpoint_hash.as_deref(),
    ) {
        (Some(local_parent), Some(remote_parent)) if local_parent == remote_parent => {
            Some(local_parent.to_owned())
        }
        _ => None,
    };
    Ok(SaveSyncResolution::Conflict {
        local_checkpoint_hash: local.checkpoint_hash.clone(),
        remote_checkpoint_hash: remote.payload.checkpoint_hash.clone(),
        common_parent_hash,
    })
}
