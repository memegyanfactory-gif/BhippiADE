//! Persistent storage for the Project Brain (Phase B).
//!
//! Higher-level indexing and retrieval logic lives in `bhippi-memory`.
//! `BrainRepo` provides storage primitives and the atomic reconcile transaction.

use crate::db_error;
use crate::repositories::RepoDb;
use bhippi_types::{
    AssetId, EntityId, FileId, ModuleId, ProjectId, Result, SceneId, SymbolId, Timestamp,
};
use std::collections::HashMap;
use std::str::FromStr;

#[derive(Clone, Debug)]
pub struct FileScan {
    pub file_id: Option<FileId>,
    pub rel_path: String,
    pub content_hash: String,
    pub source_revision: i64,
}

#[derive(Clone, Debug)]
pub struct SymbolRecord {
    pub id: SymbolId,
    pub file_id: FileId,
    pub kind: String,
    pub name: String,
    pub qualified_name: String,
    pub signature: Option<String>,
    pub start_line: Option<i64>,
    pub end_line: Option<i64>,
    pub content_hash: String,
    pub source_revision: i64,
    pub stale: bool,
    pub embedding_blob: Option<Vec<u8>>,
    pub embedding_dim: Option<i64>,
    pub embedding_model: Option<String>,
}

#[derive(Clone, Debug)]
pub struct SymbolAssignment {
    pub id: SymbolId,
    pub file_id: FileId,
    pub project_id: ProjectId,
}

/// A stored module knowledge card (Phase B8).  Facts are deterministic data derived
/// from the symbol index; any AI-generated description lives in `description` with a
/// provenance marker in `description_origin` so it can never be confused with hard
/// facts.  `card_revision` is the max symbol source_revision the card was built at.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModuleCardRecord {
    pub module_name: String,
    pub entry_points: Vec<String>,
    pub public_symbols: Vec<String>,
    pub symbol_count: i64,
    pub description: Option<String>,
    pub description_origin: Option<String>,
    pub card_revision: i64,
}

/// A persisted scene row (World Brain, ADR-0024). Stable engine facts only — no
/// business logic lives here.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SceneRecord {
    pub project_id: ProjectId,
    pub scene_id: SceneId,
    pub rel_path: String,
    pub name: String,
    pub kind: String,
    pub entity_count: i64,
    pub settings_json: String,
    pub source_revision: i64,
}

/// A persisted entity row (World Brain, ADR-0024). Component payloads stay as
/// deterministic JSON; decode with serde_json at the edge.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EntityRecord {
    pub entity_id: EntityId,
    pub scene_id: SceneId,
    pub name: String,
    pub parent_id: Option<EntityId>,
    pub tags_json: String,
    pub component_names_json: String,
    pub component_json: String,
    pub source_revision: i64,
}

/// A persisted asset row (World Brain asset graph, ADR-0025). Mirror of the engine's
/// `AssetRecord`: stable engine facts only, with reverse usage stored as quoted scene
/// ids in JSON so "what uses this asset?" is a lookup.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AssetRecord {
    pub asset_id: AssetId,
    pub project_id: ProjectId,
    pub rel_path: String,
    pub kind: String,
    pub hash: String,
    pub license: String,
    pub size_bytes: i64,
    pub used_by_scenes_json: String,
    pub source_revision: i64,
}

/// A persisted physics body/collider row (World Brain physics graph, ADR-0026). A
/// projection of the `RigidBody` / `Collider` / `CharacterController` components an
/// entity carries; `kind`/`shape` stay as authored so decoding happens at the edge.
#[derive(Clone, Debug, PartialEq)]
pub struct PhysicsBodyRecord {
    pub entity_id: EntityId,
    pub project_id: ProjectId,
    pub scene_id: SceneId,
    pub body_kind: Option<String>,
    pub mass: Option<f64>,
    pub lock_rotation: Option<i64>,
    pub collider_shape: Option<String>,
    pub sensor: Option<i64>,
    pub has_character_controller: bool,
    pub extras_json: String,
    pub source_revision: i64,
}

#[derive(Clone)]
pub struct BrainRepo {
    db: RepoDb,
}

impl BrainRepo {
    pub(crate) const fn new(db: RepoDb) -> Self {
        Self { db }
    }

    // ── projects ────────────────────────────────────────────────────────

    pub async fn count_projects(&self) -> Result<u64> {
        let count = sqlx::query_scalar!("SELECT COUNT(*) FROM brain_projects")
            .fetch_one(&self.db.readers)
            .await
            .map_err(|error| db_error(error, "count brain projects"))?;
        u64::try_from(count).map_err(|error| bhippi_types::BhippiError::Db {
            reason: format!("count brain projects: {error}"),
            retryable: false,
            hint: Some("Run `bhippi doctor` to inspect database integrity.".to_owned()),
        })
    }

    pub async fn upsert_project(
        &self,
        project_id: ProjectId,
        path: &str,
        now: &Timestamp,
    ) -> Result<()> {
        let id = project_id.to_string();
        let now = now.to_rfc3339();
        sqlx::query!(
            r#"INSERT INTO brain_projects (id, path, source_revision, created_at, updated_at)
               VALUES (?, ?, 0, ?, ?)
               ON CONFLICT(id) DO UPDATE SET
                 path = excluded.path, updated_at = excluded.updated_at"#,
            id,
            path,
            now,
            now
        )
        .execute(&self.db.writer)
        .await
        .map_err(|error| db_error(error, "upsert brain project"))?;
        Ok(())
    }

    pub async fn project_revision(&self, project_id: ProjectId) -> Result<i64> {
        let id = project_id.to_string();
        let row = sqlx::query!(
            "SELECT source_revision FROM brain_projects WHERE id = ?",
            id
        )
        .fetch_optional(&self.db.readers)
        .await
        .map_err(|error| db_error(error, "read brain project revision"))?;
        Ok(row.map_or(0, |r| r.source_revision))
    }

    pub async fn project_by_path(&self, path: &str) -> Result<Option<ProjectId>> {
        let row = sqlx::query!("SELECT id FROM brain_projects WHERE path = ?", path)
            .fetch_optional(&self.db.readers)
            .await
            .map_err(|error| db_error(error, "lookup brain project by path"))?;
        Ok(row.and_then(|r| r.id.as_deref().and_then(|s| ProjectId::from_str(s).ok())))
    }

    pub async fn bump_project_revision(
        &self,
        project_id: ProjectId,
        now: &Timestamp,
    ) -> Result<i64> {
        let id = project_id.to_string();
        let now = now.to_rfc3339();
        sqlx::query!(
            "UPDATE brain_projects SET source_revision = source_revision + 1, updated_at = ? WHERE id = ?",
            now,
            id
        )
        .execute(&self.db.writer)
        .await
        .map_err(|error| db_error(error, "bump brain project revision"))?;
        let row = sqlx::query!(
            "SELECT source_revision FROM brain_projects WHERE id = ?",
            id
        )
        .fetch_one(&self.db.readers)
        .await
        .map_err(|error| db_error(error, "read bumped brain project revision"))?;
        Ok(row.source_revision)
    }

    // ── modules ─────────────────────────────────────────────────────────

    pub async fn upsert_module(
        &self,
        module_id: ModuleId,
        project_id: ProjectId,
        name: &str,
        now: &Timestamp,
    ) -> Result<()> {
        let id = module_id.to_string();
        let project = project_id.to_string();
        let now = now.to_rfc3339();
        sqlx::query!(
            r#"INSERT INTO brain_modules (id, project_id, name, created_at, updated_at)
               VALUES (?, ?, ?, ?, ?)
               ON CONFLICT(project_id, name) DO UPDATE SET updated_at = excluded.updated_at"#,
            id,
            project,
            name,
            now,
            now
        )
        .execute(&self.db.writer)
        .await
        .map_err(|error| db_error(error, "upsert brain module"))?;
        Ok(())
    }

    pub async fn module_names(&self, project_id: ProjectId) -> Result<Vec<String>> {
        let project = project_id.to_string();
        let rows = sqlx::query_scalar!(
            "SELECT name FROM brain_modules WHERE project_id = ?",
            project
        )
        .fetch_all(&self.db.readers)
        .await
        .map_err(|error| db_error(error, "list brain modules"))?;
        Ok(rows)
    }

    // ── files ───────────────────────────────────────────────────────────

    pub async fn file_scan(
        &self,
        project_id: ProjectId,
        rel_path: &str,
    ) -> Result<Option<FileScan>> {
        let project = project_id.to_string();
        let row = sqlx::query!(
            "SELECT id, content_hash, source_revision FROM brain_files WHERE project_id = ? AND rel_path = ?",
            project,
            rel_path
        )
        .fetch_optional(&self.db.readers)
        .await
        .map_err(|error| db_error(error, "read brain file scan"))?;
        Ok(row.map(|r| FileScan {
            file_id: r.id.as_deref().and_then(|s| FileId::from_str(s).ok()),
            rel_path: rel_path.to_owned(),
            content_hash: r.content_hash,
            source_revision: r.source_revision,
        }))
    }

    pub async fn upsert_file(
        &self,
        file_id: FileId,
        project_id: ProjectId,
        rel_path: &str,
        content_hash: &str,
        source_revision: i64,
        now: &Timestamp,
    ) -> Result<()> {
        let id = file_id.to_string();
        let project = project_id.to_string();
        let now = now.to_rfc3339();
        sqlx::query!(
            r#"INSERT INTO brain_files
                 (id, project_id, rel_path, content_hash, source_revision,
                  created_at, updated_at, stale)
               VALUES (?, ?, ?, ?, ?, ?, ?, 0)
               ON CONFLICT(project_id, rel_path) DO UPDATE SET
                 id = excluded.id,
                 content_hash = excluded.content_hash,
                 source_revision = excluded.source_revision,
                 updated_at = excluded.updated_at,
                 stale = 0"#,
            id,
            project,
            rel_path,
            content_hash,
            source_revision,
            now,
            now
        )
        .execute(&self.db.writer)
        .await
        .map_err(|error| db_error(error, "upsert brain file"))?;
        Ok(())
    }

    pub async fn mark_file_stale(
        &self,
        project_id: ProjectId,
        rel_path: &str,
        now: &Timestamp,
    ) -> Result<()> {
        let project = project_id.to_string();
        let now = now.to_rfc3339();
        sqlx::query!(
            "UPDATE brain_files SET stale = 1, updated_at = ? WHERE project_id = ? AND rel_path = ?",
            now,
            project,
            rel_path
        )
        .execute(&self.db.writer)
        .await
        .map_err(|error| db_error(error, "mark brain file stale"))?;
        Ok(())
    }

    // ── symbols ─────────────────────────────────────────────────────────

    pub async fn upsert_symbol(
        &self,
        symbol: &SymbolRecord,
        project_id: ProjectId,
        parent_id: Option<SymbolId>,
        now: &Timestamp,
    ) -> Result<()> {
        let id = symbol.id.to_string();
        let project = project_id.to_string();
        let file = symbol.file_id.to_string();
        let now = now.to_rfc3339();
        let parent = parent_id.map(|v| v.to_string());
        sqlx::query!(
            r#"INSERT INTO brain_symbols
                 (id, project_id, file_id, kind, name, qualified_name, signature,
                  start_line, end_line, content_hash, source_revision,
                  parent_symbol, source_of_truth, created_at, updated_at, stale,
                  embedding_blob, embedding_dim, embedding_model)
               VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 'index', ?, ?, 0, ?, ?, ?)
               ON CONFLICT(id) DO UPDATE SET
                 project_id = excluded.project_id,
                 file_id = excluded.file_id,
                 kind = excluded.kind,
                 signature = excluded.signature,
                 start_line = excluded.start_line,
                 end_line = excluded.end_line,
                 content_hash = excluded.content_hash,
                 source_revision = excluded.source_revision,
                 parent_symbol = excluded.parent_symbol,
                 updated_at = excluded.updated_at,
                 stale = 0,
                 embedding_blob = excluded.embedding_blob,
                 embedding_dim = excluded.embedding_dim,
                 embedding_model = excluded.embedding_model"#,
            id,
            project,
            file,
            symbol.kind,
            symbol.name,
            symbol.qualified_name,
            symbol.signature,
            symbol.start_line,
            symbol.end_line,
            symbol.content_hash,
            symbol.source_revision,
            parent,
            now,
            now,
            symbol.embedding_blob,
            symbol.embedding_dim,
            symbol.embedding_model
        )
        .execute(&self.db.writer)
        .await
        .map_err(|error| db_error(error, "upsert brain symbol"))?;
        Ok(())
    }

    /// Atomically mark all symbols for a file as stale, then un-stale the ones still
    /// present in the new revision.
    pub async fn reconcile_symbols(
        &self,
        project_id: ProjectId,
        file_id: FileId,
        seen: &[SymbolId],
        now: &Timestamp,
    ) -> Result<()> {
        let project = project_id.to_string();
        let file = file_id.to_string();
        let now = now.to_rfc3339();
        let mut tx = self
            .db
            .writer
            .begin()
            .await
            .map_err(|error| db_error(error, "begin brain symbol reconcile"))?;
        sqlx::query!(
            "UPDATE brain_symbols SET stale = 1, updated_at = ? WHERE project_id = ? AND file_id = ?",
            now,
            project,
            file
        )
        .execute(&mut *tx)
        .await
        .map_err(|error| db_error(error, "stale brain symbols"))?;
        for symbol_id in seen {
            let sid = symbol_id.to_string();
            sqlx::query!(
                "UPDATE brain_symbols SET stale = 0, updated_at = ? WHERE id = ?",
                now,
                sid
            )
            .execute(&mut *tx)
            .await
            .map_err(|error| db_error(error, "refresh brain symbol"))?;
        }
        tx.commit()
            .await
            .map_err(|error| db_error(error, "commit brain symbol reconcile"))
    }

    pub async fn symbols_for_file(
        &self,
        project_id: ProjectId,
        file_id: FileId,
    ) -> Result<Vec<SymbolRecord>> {
        let project = project_id.to_string();
        let file = file_id.to_string();
        let rows = sqlx::query!(
            r#"SELECT id, file_id, kind, name, qualified_name, signature,
                      start_line, end_line, content_hash, source_revision, stale,
                      embedding_blob, embedding_dim, embedding_model
               FROM brain_symbols
               WHERE project_id = ? AND file_id = ? AND stale = 0"#,
            project,
            file
        )
        .fetch_all(&self.db.readers)
        .await
        .map_err(|error| db_error(error, "read brain symbols for file"))?;
        Ok(rows
            .into_iter()
            .filter_map(|row| {
                Some(SymbolRecord {
                    id: SymbolId::from_str(row.id.as_deref()?).ok()?,
                    file_id: FileId::from_str(&row.file_id).ok()?,
                    kind: row.kind,
                    name: row.name,
                    qualified_name: row.qualified_name,
                    signature: row.signature,
                    start_line: row.start_line,
                    end_line: row.end_line,
                    content_hash: row.content_hash,
                    source_revision: row.source_revision,
                    stale: row.stale != 0,
                    embedding_blob: row.embedding_blob,
                    embedding_dim: row.embedding_dim,
                    embedding_model: row.embedding_model,
                })
            })
            .collect())
    }

    pub async fn symbol_by_qualified(
        &self,
        project_id: ProjectId,
        qualified_name: &str,
    ) -> Result<Option<SymbolRecord>> {
        let project = project_id.to_string();
        let row = sqlx::query!(
            r#"SELECT id, file_id, kind, name, qualified_name, signature,
                      start_line, end_line, content_hash, source_revision, stale,
                      embedding_blob, embedding_dim, embedding_model
               FROM brain_symbols
               WHERE project_id = ? AND qualified_name = ? AND stale = 0"#,
            project,
            qualified_name
        )
        .fetch_optional(&self.db.readers)
        .await
        .map_err(|error| db_error(error, "read brain symbol by qualified name"))?;
        Ok(row.and_then(|row| {
            Some(SymbolRecord {
                id: SymbolId::from_str(row.id.as_deref()?).ok()?,
                file_id: FileId::from_str(&row.file_id).ok()?,
                kind: row.kind,
                name: row.name,
                qualified_name: row.qualified_name,
                signature: row.signature,
                start_line: row.start_line,
                end_line: row.end_line,
                content_hash: row.content_hash,
                source_revision: row.source_revision,
                stale: row.stale != 0,
                embedding_blob: row.embedding_blob,
                embedding_dim: row.embedding_dim,
                embedding_model: row.embedding_model,
            })
        }))
    }

    pub async fn count_symbols(&self, project_id: ProjectId) -> Result<u64> {
        let project = project_id.to_string();
        let count = sqlx::query_scalar!(
            "SELECT COUNT(*) FROM brain_symbols WHERE project_id = ? AND stale = 0",
            project
        )
        .fetch_one(&self.db.readers)
        .await
        .map_err(|error| db_error(error, "count brain symbols"))?;
        u64::try_from(count).map_err(|error| bhippi_types::BhippiError::Db {
            reason: format!("count brain symbols: {error}"),
            retryable: false,
            hint: Some("Run `bhippi doctor` to inspect database integrity.".to_owned()),
        })
    }

    pub async fn symbol_assignments(
        &self,
        project_id: ProjectId,
    ) -> Result<HashMap<String, Vec<SymbolAssignment>>> {
        let project = project_id.to_string();
        let rows = sqlx::query!(
            r#"SELECT s.id, s.file_id, f.rel_path
               FROM brain_symbols s
               JOIN brain_files f ON f.id = s.file_id
               WHERE s.project_id = ? AND s.stale = 0"#,
            project
        )
        .fetch_all(&self.db.readers)
        .await
        .map_err(|error| db_error(error, "read brain symbol assignments"))?;
        let mut map: HashMap<String, Vec<SymbolAssignment>> = HashMap::new();
        for row in rows {
            let Some(id_str) = row.id.as_deref() else {
                continue;
            };
            let file_str = &row.file_id;
            let Some(id) = SymbolId::from_str(id_str).ok() else {
                continue;
            };
            let Some(file_id) = FileId::from_str(file_str).ok() else {
                continue;
            };
            map.entry(row.rel_path).or_default().push(SymbolAssignment {
                id,
                file_id,
                project_id,
            });
        }
        Ok(map)
    }

    // ── embedding state (Phase B5) ──────────────────────────────────────

    /// Record which embedding model/version a project's index was last built with.
    pub async fn set_embedding_state(
        &self,
        project_id: ProjectId,
        model: &str,
        now: &Timestamp,
    ) -> Result<()> {
        let project = project_id.to_string();
        let now = now.to_rfc3339();
        sqlx::query!(
            r#"INSERT INTO brain_embedding_state (project_id, model, updated_at)
               VALUES (?, ?, ?)
               ON CONFLICT(project_id) DO UPDATE SET
                 model = excluded.model, updated_at = excluded.updated_at"#,
            project,
            model,
            now
        )
        .execute(&self.db.writer)
        .await
        .map_err(|error| db_error(error, "set brain embedding state"))?;
        Ok(())
    }

    /// The embedding model id a project's index was last built with, if any.
    pub async fn embedding_model(&self, project_id: ProjectId) -> Result<Option<String>> {
        let project = project_id.to_string();
        let row = sqlx::query!(
            "SELECT model FROM brain_embedding_state WHERE project_id = ?",
            project
        )
        .fetch_optional(&self.db.readers)
        .await
        .map_err(|error| db_error(error, "read brain embedding state"))?;
        Ok(row.map(|r| r.model))
    }

    /// Reuse an existing, still-live embedding for a symbol `(file_id,
    /// qualified_name, content_hash)`.  Returns the stored blob when an identical
    /// non-stale symbol already carries an embedding, so a rescan never recomputes
    /// unchanged chunks (plan SEC. 5: "re-embed only changed chunks").
    pub async fn identical_symbol_embedding(
        &self,
        project_id: ProjectId,
        file_id: FileId,
        qualified_name: &str,
        content_hash: &str,
    ) -> Result<Option<Vec<u8>>> {
        let project = project_id.to_string();
        let file = file_id.to_string();
        let row = sqlx::query!(
            r#"SELECT embedding_blob
               FROM brain_symbols
               WHERE project_id = ? AND file_id = ? AND qualified_name = ?
                 AND content_hash = ? AND stale = 0 AND embedding_blob IS NOT NULL
               ORDER BY updated_at DESC LIMIT 1"#,
            project,
            file,
            qualified_name,
            content_hash
        )
        .fetch_optional(&self.db.readers)
        .await
        .map_err(|error| db_error(error, "read reusable symbol embedding"))?;
        Ok(row.and_then(|r| r.embedding_blob))
    }

    /// All non-stale symbols in a project that carry an embedding, for search.
    pub async fn symbols_with_embeddings(
        &self,
        project_id: ProjectId,
    ) -> Result<Vec<SymbolRecord>> {
        let project = project_id.to_string();
        let rows = sqlx::query!(
            r#"SELECT id, file_id, kind, name, qualified_name, signature,
                      start_line, end_line, content_hash, source_revision, stale,
                      embedding_blob, embedding_dim, embedding_model
               FROM brain_symbols
               WHERE project_id = ? AND stale = 0 AND embedding_blob IS NOT NULL"#,
            project
        )
        .fetch_all(&self.db.readers)
        .await
        .map_err(|error| db_error(error, "read brain symbols with embeddings"))?;
        Ok(rows
            .into_iter()
            .filter_map(|row| {
                Some(SymbolRecord {
                    id: SymbolId::from_str(row.id.as_deref()?).ok()?,
                    file_id: FileId::from_str(&row.file_id).ok()?,
                    kind: row.kind,
                    name: row.name,
                    qualified_name: row.qualified_name,
                    signature: row.signature,
                    start_line: row.start_line,
                    end_line: row.end_line,
                    content_hash: row.content_hash,
                    source_revision: row.source_revision,
                    stale: row.stale != 0,
                    embedding_blob: row.embedding_blob,
                    embedding_dim: row.embedding_dim,
                    embedding_model: row.embedding_model,
                })
            })
            .collect())
    }

    /// Rel paths of every non-stale file currently tracked for a project.
    /// Used by the incremental reindexer to detect files that have disappeared.
    pub async fn file_paths(&self, project_id: ProjectId) -> Result<Vec<String>> {
        let project = project_id.to_string();
        let rows = sqlx::query_scalar!(
            "SELECT rel_path FROM brain_files WHERE project_id = ? AND stale = 0",
            project
        )
        .fetch_all(&self.db.readers)
        .await
        .map_err(|error| db_error(error, "list brain file paths"))?;
        Ok(rows)
    }

    // ── module cards (Phase B8) ─────────────────────────────────────────

    fn decode_card_json(value: String) -> Vec<String> {
        serde_json::from_str(&value).unwrap_or_default()
    }

    pub async fn get_module_card(
        &self,
        project_id: ProjectId,
        module_name: &str,
    ) -> Result<Option<ModuleCardRecord>> {
        let project = project_id.to_string();
        let row = sqlx::query!(
            r#"SELECT entry_points, public_symbols, symbol_count, description,
                      description_origin, card_revision
               FROM brain_module_cards
               WHERE project_id = ? AND module_name = ?"#,
            project,
            module_name
        )
        .fetch_optional(&self.db.readers)
        .await
        .map_err(|error| db_error(error, "read brain module card"))?;
        Ok(row.map(|r| ModuleCardRecord {
            module_name: module_name.to_owned(),
            entry_points: Self::decode_card_json(r.entry_points),
            public_symbols: Self::decode_card_json(r.public_symbols),
            symbol_count: r.symbol_count,
            description: r.description,
            description_origin: r.description_origin,
            card_revision: r.card_revision,
        }))
    }

    pub async fn all_module_cards(&self, project_id: ProjectId) -> Result<Vec<ModuleCardRecord>> {
        let project = project_id.to_string();
        let rows = sqlx::query!(
            r#"SELECT module_name, entry_points, public_symbols, symbol_count, description,
                      description_origin, card_revision
               FROM brain_module_cards WHERE project_id = ?"#,
            project
        )
        .fetch_all(&self.db.readers)
        .await
        .map_err(|error| db_error(error, "list brain module cards"))?;
        Ok(rows
            .into_iter()
            .map(|r| ModuleCardRecord {
                module_name: r.module_name,
                entry_points: Self::decode_card_json(r.entry_points),
                public_symbols: Self::decode_card_json(r.public_symbols),
                symbol_count: r.symbol_count,
                description: r.description,
                description_origin: r.description_origin,
                card_revision: r.card_revision,
            })
            .collect())
    }

    pub async fn upsert_module_card(
        &self,
        project_id: ProjectId,
        card: &ModuleCardRecord,
        now: &Timestamp,
    ) -> Result<()> {
        let project = project_id.to_string();
        let now = now.to_rfc3339();
        let entry_points = serde_json::to_string(&card.entry_points).map_err(|error| {
            bhippi_types::BhippiError::Db {
                reason: format!("serialize module entry points: {error}"),
                retryable: false,
                hint: Some("Run `bhippi doctor` to inspect database integrity.".to_owned()),
            }
        })?;
        let public_symbols = serde_json::to_string(&card.public_symbols).map_err(|error| {
            bhippi_types::BhippiError::Db {
                reason: format!("serialize module public symbols: {error}"),
                retryable: false,
                hint: Some("Run `bhippi doctor` to inspect database integrity.".to_owned()),
            }
        })?;
        sqlx::query!(
            r#"INSERT INTO brain_module_cards
                 (project_id, module_name, entry_points, public_symbols, symbol_count,
                  description, description_origin, card_revision, updated_at)
               VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
               ON CONFLICT(project_id, module_name) DO UPDATE SET
                 entry_points = excluded.entry_points,
                 public_symbols = excluded.public_symbols,
                 symbol_count = excluded.symbol_count,
                 description = excluded.description,
                 description_origin = excluded.description_origin,
                 card_revision = excluded.card_revision,
                 updated_at = excluded.updated_at"#,
            project,
            card.module_name,
            entry_points,
            public_symbols,
            card.symbol_count,
            card.description,
            card.description_origin,
            card.card_revision,
            now
        )
        .execute(&self.db.writer)
        .await
        .map_err(|error| db_error(error, "upsert brain module card"))?;
        Ok(())
    }

    // ── world brain scenes (Phase B9 / ADR-0024) ───────────────────────

    pub async fn scene_by_path(
        &self,
        project_id: ProjectId,
        rel_path: &str,
    ) -> Result<Option<SceneRecord>> {
        let project = project_id.to_string();
        let row = sqlx::query!(
            r#"SELECT scene_id, rel_path, name, kind, entity_count, settings_json, source_revision
               FROM brain_scenes WHERE project_id = ? AND rel_path = ?"#,
            project,
            rel_path
        )
        .fetch_optional(&self.db.readers)
        .await
        .map_err(|error| db_error(error, "lookup brain scene by path"))?;
        Ok(row.and_then(|r| {
            let scene_id = SceneId::from_str(r.scene_id.as_deref()?).ok()?;
            Some(SceneRecord {
                project_id,
                scene_id,
                rel_path: r.rel_path,
                name: r.name,
                kind: r.kind,
                entity_count: r.entity_count,
                settings_json: r.settings_json,
                source_revision: r.source_revision,
            })
        }))
    }

    pub async fn scene_by_id(
        &self,
        project_id: ProjectId,
        scene_id: SceneId,
    ) -> Result<Option<SceneRecord>> {
        let project = project_id.to_string();
        let scene = scene_id.to_string();
        let row = sqlx::query!(
            r#"SELECT scene_id, rel_path, name, kind, entity_count, settings_json, source_revision
               FROM brain_scenes WHERE project_id = ? AND scene_id = ?"#,
            project,
            scene
        )
        .fetch_optional(&self.db.readers)
        .await
        .map_err(|error| db_error(error, "lookup brain scene by id"))?;
        Ok(row.map(|r| SceneRecord {
            project_id,
            scene_id,
            rel_path: r.rel_path,
            name: r.name,
            kind: r.kind,
            entity_count: r.entity_count,
            settings_json: r.settings_json,
            source_revision: r.source_revision,
        }))
    }

    pub async fn list_scenes(&self, project_id: ProjectId) -> Result<Vec<SceneRecord>> {
        let project = project_id.to_string();
        let rows = sqlx::query!(
            r#"SELECT scene_id, rel_path, name, kind, entity_count, settings_json, source_revision
               FROM brain_scenes WHERE project_id = ? ORDER BY rel_path"#,
            project
        )
        .fetch_all(&self.db.readers)
        .await
        .map_err(|error| db_error(error, "list brain scenes"))?;
        let mut out = Vec::with_capacity(rows.len());
        for row in rows {
            if let Some(scene_id) = row
                .scene_id
                .as_deref()
                .and_then(|s| SceneId::from_str(s).ok())
            {
                out.push(SceneRecord {
                    project_id,
                    scene_id,
                    rel_path: row.rel_path,
                    name: row.name,
                    kind: row.kind,
                    entity_count: row.entity_count,
                    settings_json: row.settings_json,
                    source_revision: row.source_revision,
                });
            }
        }
        Ok(out)
    }

    pub async fn upsert_scene(
        &self,
        project_id: ProjectId,
        record: &SceneRecord,
        now: &Timestamp,
    ) -> Result<()> {
        let project = project_id.to_string();
        let scene_id = record.scene_id.to_string();
        let now = now.to_rfc3339();
        sqlx::query!(
            r#"INSERT INTO brain_scenes
                 (project_id, scene_id, rel_path, name, kind, entity_count, settings_json,
                  source_revision, created_at, updated_at)
               VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
               ON CONFLICT(scene_id) DO UPDATE SET
                 project_id = excluded.project_id,
                 rel_path = excluded.rel_path,
                 name = excluded.name,
                 kind = excluded.kind,
                 entity_count = excluded.entity_count,
                 settings_json = excluded.settings_json,
                 source_revision = excluded.source_revision,
                 updated_at = excluded.updated_at"#,
            project,
            scene_id,
            record.rel_path,
            record.name,
            record.kind,
            record.entity_count,
            record.settings_json,
            record.source_revision,
            now,
            now
        )
        .execute(&self.db.writer)
        .await
        .map_err(|error| db_error(error, "upsert brain scene"))?;
        Ok(())
    }

    /// Remove all entities of a scene, then insert `records`. Runs in one transaction
    /// and inserts parents before children so the `parent_id` FK holds.
    pub async fn replace_scene_entities(
        &self,
        project_id: ProjectId,
        scene_id: SceneId,
        records: &[EntityRecord],
        now: &Timestamp,
    ) -> Result<()> {
        let project = project_id.to_string();
        let scene = scene_id.to_string();
        let now = now.to_rfc3339();
        let mut tx =
            self.db
                .writer
                .begin()
                .await
                .map_err(|error| bhippi_types::BhippiError::Db {
                    reason: format!("begin world brain entity replace: {error}"),
                    retryable: false,
                    hint: Some("Run `bhippi doctor` and retry.".to_owned()),
                })?;
        sqlx::query!("DELETE FROM brain_entities WHERE scene_id = ?", scene)
            .execute(&mut *tx)
            .await
            .map_err(|error| db_error(error, "clear brain scene entities"))?;
        // Insert parents before children: a scene's parent always appears earlier in
        // authoring order, but sort defensively so the FK never rejects a child.
        let mut sorted = records.to_vec();
        sorted.sort_by_key(|r| r.parent_id.is_some());
        for record in &sorted {
            let entity_id = record.entity_id.to_string();
            let parent = record.parent_id.map(|id| id.to_string());
            sqlx::query!(
                r#"INSERT INTO brain_entities
                     (entity_id, project_id, scene_id, name, parent_id, tags_json,
                      component_names_json, component_json, source_revision, created_at, updated_at)
                   VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
                entity_id,
                project,
                scene,
                record.name,
                parent,
                record.tags_json,
                record.component_names_json,
                record.component_json,
                record.source_revision,
                now,
                now
            )
            .execute(&mut *tx)
            .await
            .map_err(|error| db_error(error, "insert brain scene entity"))?;
        }
        tx.commit()
            .await
            .map_err(|error| bhippi_types::BhippiError::Db {
                reason: format!("commit world brain entity replace: {error}"),
                retryable: false,
                hint: Some("Run `bhippi doctor` and retry.".to_owned()),
            })
    }

    pub async fn scene_entities(&self, scene_id: SceneId) -> Result<Vec<EntityRecord>> {
        let scene = scene_id.to_string();
        let rows = sqlx::query!(
            r#"SELECT entity_id, scene_id, name, parent_id, tags_json, component_names_json,
                      component_json, source_revision
               FROM brain_entities WHERE scene_id = ? ORDER BY rowid"#,
            scene
        )
        .fetch_all(&self.db.readers)
        .await
        .map_err(|error| db_error(error, "list brain scene entities"))?;
        let mut out = Vec::with_capacity(rows.len());
        for row in rows {
            let Some(entity_id) = row
                .entity_id
                .as_deref()
                .and_then(|s| EntityId::from_str(s).ok())
            else {
                continue;
            };
            let Some(scene_id) = SceneId::from_str(&row.scene_id).ok() else {
                continue;
            };
            let parent_id = row
                .parent_id
                .as_deref()
                .and_then(|s| EntityId::from_str(s).ok());
            out.push(EntityRecord {
                entity_id,
                scene_id,
                name: row.name,
                parent_id,
                tags_json: row.tags_json,
                component_names_json: row.component_names_json,
                component_json: row.component_json,
                source_revision: row.source_revision,
            });
        }
        Ok(out)
    }

    pub async fn entity_by_id(&self, entity_id: EntityId) -> Result<Option<EntityRecord>> {
        let id = entity_id.to_string();
        let row = sqlx::query!(
            r#"SELECT entity_id, scene_id, name, parent_id, tags_json, component_names_json,
                      component_json, source_revision
               FROM brain_entities WHERE entity_id = ?"#,
            id
        )
        .fetch_optional(&self.db.readers)
        .await
        .map_err(|error| db_error(error, "lookup brain scene entity"))?;
        Ok(row.and_then(|r| {
            let scene_id = SceneId::from_str(&r.scene_id).ok()?;
            Some(EntityRecord {
                entity_id,
                scene_id,
                name: r.name,
                parent_id: r
                    .parent_id
                    .as_deref()
                    .and_then(|s| EntityId::from_str(s).ok()),
                tags_json: r.tags_json,
                component_names_json: r.component_names_json,
                component_json: r.component_json,
                source_revision: r.source_revision,
            })
        }))
    }

    pub async fn remove_scene(&self, scene_id: SceneId) -> Result<()> {
        let scene = scene_id.to_string();
        sqlx::query!("DELETE FROM brain_scenes WHERE scene_id = ?", scene)
            .execute(&self.db.writer)
            .await
            .map_err(|error| db_error(error, "remove brain scene"))?;
        Ok(())
    }

    // ── world brain assets (ADR-0025, plan SEC. 7.2) ──────────────────────

    pub async fn asset_by_path(
        &self,
        project_id: ProjectId,
        rel_path: &str,
    ) -> Result<Option<AssetRecord>> {
        let project = project_id.to_string();
        let row = sqlx::query!(
            r#"SELECT asset_id, rel_path, kind, hash, license, size_bytes,
                      used_by_scenes_json, source_revision
               FROM brain_assets WHERE project_id = ? AND rel_path = ?"#,
            project,
            rel_path
        )
        .fetch_optional(&self.db.readers)
        .await
        .map_err(|error| db_error(error, "lookup brain asset by path"))?;
        Ok(row.and_then(|r| {
            let asset_id = AssetId::from_str(r.asset_id.as_deref()?).ok()?;
            Some(AssetRecord {
                asset_id,
                project_id,
                rel_path: r.rel_path,
                kind: r.kind,
                hash: r.hash,
                license: r.license,
                size_bytes: r.size_bytes,
                used_by_scenes_json: r.used_by_scenes_json,
                source_revision: r.source_revision,
            })
        }))
    }

    pub async fn asset_by_id(
        &self,
        project_id: ProjectId,
        asset_id: AssetId,
    ) -> Result<Option<AssetRecord>> {
        let project = project_id.to_string();
        let asset = asset_id.to_string();
        let row = sqlx::query!(
            r#"SELECT asset_id, rel_path, kind, hash, license, size_bytes,
                      used_by_scenes_json, source_revision
               FROM brain_assets WHERE project_id = ? AND asset_id = ?"#,
            project,
            asset
        )
        .fetch_optional(&self.db.readers)
        .await
        .map_err(|error| db_error(error, "lookup brain asset by id"))?;
        Ok(row.map(|r| AssetRecord {
            asset_id,
            project_id,
            rel_path: r.rel_path,
            kind: r.kind,
            hash: r.hash,
            license: r.license,
            size_bytes: r.size_bytes,
            used_by_scenes_json: r.used_by_scenes_json,
            source_revision: r.source_revision,
        }))
    }

    pub async fn assets_by_project(&self, project_id: ProjectId) -> Result<Vec<AssetRecord>> {
        let project = project_id.to_string();
        let rows = sqlx::query!(
            r#"SELECT asset_id, rel_path, kind, hash, license, size_bytes,
                      used_by_scenes_json, source_revision
               FROM brain_assets WHERE project_id = ? ORDER BY rel_path"#,
            project
        )
        .fetch_all(&self.db.readers)
        .await
        .map_err(|error| db_error(error, "list brain assets"))?;
        let mut out = Vec::with_capacity(rows.len());
        for row in rows {
            if let Some(asset_id) = row
                .asset_id
                .as_deref()
                .and_then(|s| AssetId::from_str(s).ok())
            {
                out.push(AssetRecord {
                    asset_id,
                    project_id,
                    rel_path: row.rel_path,
                    kind: row.kind,
                    hash: row.hash,
                    license: row.license,
                    size_bytes: row.size_bytes,
                    used_by_scenes_json: row.used_by_scenes_json,
                    source_revision: row.source_revision,
                });
            }
        }
        Ok(out)
    }

    pub async fn assets_by_kind(
        &self,
        project_id: ProjectId,
        kind: &str,
    ) -> Result<Vec<AssetRecord>> {
        let project = project_id.to_string();
        let rows = sqlx::query!(
            r#"SELECT asset_id, rel_path, kind, hash, license, size_bytes,
                      used_by_scenes_json, source_revision
               FROM brain_assets WHERE project_id = ? AND kind = ? ORDER BY rel_path"#,
            project,
            kind
        )
        .fetch_all(&self.db.readers)
        .await
        .map_err(|error| db_error(error, "list brain assets by kind"))?;
        let mut out = Vec::with_capacity(rows.len());
        for row in rows {
            if let Some(asset_id) = row
                .asset_id
                .as_deref()
                .and_then(|s| AssetId::from_str(s).ok())
            {
                out.push(AssetRecord {
                    asset_id,
                    project_id,
                    rel_path: row.rel_path,
                    kind: row.kind,
                    hash: row.hash,
                    license: row.license,
                    size_bytes: row.size_bytes,
                    used_by_scenes_json: row.used_by_scenes_json,
                    source_revision: row.source_revision,
                });
            }
        }
        Ok(out)
    }

    /// Replace every asset row for a project with `records` in one transaction. The
    /// engine's `AssetIndex` is rebuilt wholesale on scan, so replace-all is the correct
    /// incremental unit (mirrors per-scene entity replace in ADR-0024).
    pub async fn replace_project_assets(
        &self,
        project_id: ProjectId,
        records: &[AssetRecord],
        now: &Timestamp,
    ) -> Result<()> {
        let project = project_id.to_string();
        let now = now.to_rfc3339();
        let mut tx =
            self.db
                .writer
                .begin()
                .await
                .map_err(|error| bhippi_types::BhippiError::Db {
                    reason: format!("begin world brain asset replace: {error}"),
                    retryable: false,
                    hint: Some("Run `bhippi doctor` and retry.".to_owned()),
                })?;
        sqlx::query!("DELETE FROM brain_assets WHERE project_id = ?", project)
            .execute(&mut *tx)
            .await
            .map_err(|error| db_error(error, "clear brain assets"))?;
        for record in records {
            let asset_id = record.asset_id.to_string();
            sqlx::query!(
                r#"INSERT INTO brain_assets
                     (asset_id, project_id, rel_path, kind, hash, license, size_bytes,
                      used_by_scenes_json, source_revision, created_at, updated_at)
                   VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
                asset_id,
                project,
                record.rel_path,
                record.kind,
                record.hash,
                record.license,
                record.size_bytes,
                record.used_by_scenes_json,
                record.source_revision,
                now,
                now
            )
            .execute(&mut *tx)
            .await
            .map_err(|error| db_error(error, "insert brain asset"))?;
        }
        tx.commit()
            .await
            .map_err(|error| bhippi_types::BhippiError::Db {
                reason: format!("commit world brain asset replace: {error}"),
                retryable: false,
                hint: Some("Run `bhippi doctor` and retry.".to_owned()),
            })
    }

    // ── world brain physics (ADR-0026, plan SEC. 7.3) ────────────────────

    /// Replace every physics body row for one scene with `records` in one transaction.
    /// Must run after `replace_scene_entities` for the same scene because the row FK
    /// targets `brain_entities(entity_id)`; the incremental unit mirrors the per-scene
    /// entity replace in ADR-0024.
    pub async fn replace_scene_physics(
        &self,
        project_id: ProjectId,
        scene_id: SceneId,
        records: &[PhysicsBodyRecord],
        now: &Timestamp,
    ) -> Result<()> {
        let project = project_id.to_string();
        let scene = scene_id.to_string();
        let now = now.to_rfc3339();
        let mut tx =
            self.db
                .writer
                .begin()
                .await
                .map_err(|error| bhippi_types::BhippiError::Db {
                    reason: format!("begin world brain physics replace: {error}"),
                    retryable: false,
                    hint: Some("Run `bhippi doctor` and retry.".to_owned()),
                })?;
        sqlx::query!("DELETE FROM brain_physics_bodies WHERE scene_id = ?", scene)
            .execute(&mut *tx)
            .await
            .map_err(|error| db_error(error, "clear brain scene physics"))?;
        for record in records {
            let entity_id = record.entity_id.to_string();
            let lock_rotation = record.lock_rotation.map(|v| if v == 0 { 0 } else { 1 });
            let sensor = record.sensor.map(|v| if v == 0 { 0 } else { 1 });
            let cc = if record.has_character_controller {
                1
            } else {
                0
            };
            sqlx::query!(
                r#"INSERT INTO brain_physics_bodies
                     (entity_id, project_id, scene_id, body_kind, mass, lock_rotation,
                      collider_shape, sensor, has_character_controller, extras_json,
                      source_revision, created_at, updated_at)
                   VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
                entity_id,
                project,
                scene,
                record.body_kind,
                record.mass,
                lock_rotation,
                record.collider_shape,
                sensor,
                cc,
                record.extras_json,
                record.source_revision,
                now,
                now
            )
            .execute(&mut *tx)
            .await
            .map_err(|error| db_error(error, "insert brain physics body"))?;
        }
        tx.commit()
            .await
            .map_err(|error| bhippi_types::BhippiError::Db {
                reason: format!("commit world brain physics replace: {error}"),
                retryable: false,
                hint: Some("Run `bhippi doctor` and retry.".to_owned()),
            })
    }

    pub async fn physics_bodies_by_project(
        &self,
        project_id: ProjectId,
    ) -> Result<Vec<PhysicsBodyRecord>> {
        let project = project_id.to_string();
        let rows = sqlx::query!(
            r#"SELECT entity_id, scene_id, body_kind, mass, lock_rotation, collider_shape,
                      sensor, has_character_controller, extras_json, source_revision
               FROM brain_physics_bodies WHERE project_id = ? ORDER BY rowid"#,
            project
        )
        .fetch_all(&self.db.readers)
        .await
        .map_err(|error| db_error(error, "list brain physics bodies"))?;
        let mut out = Vec::with_capacity(rows.len());
        for row in rows {
            let Some(entity_id) = row
                .entity_id
                .as_deref()
                .and_then(|s| EntityId::from_str(s).ok())
            else {
                continue;
            };
            let Some(scene_id) = SceneId::from_str(&row.scene_id).ok() else {
                continue;
            };
            out.push(PhysicsBodyRecord {
                entity_id,
                project_id,
                scene_id,
                body_kind: row.body_kind,
                mass: row.mass,
                lock_rotation: row.lock_rotation,
                collider_shape: row.collider_shape,
                sensor: row.sensor,
                has_character_controller: row.has_character_controller != 0,
                extras_json: row.extras_json,
                source_revision: row.source_revision,
            });
        }
        Ok(out)
    }

    pub async fn physics_bodies_by_scene(
        &self,
        scene_id: SceneId,
    ) -> Result<Vec<PhysicsBodyRecord>> {
        let scene = scene_id.to_string();
        let rows = sqlx::query!(
            r#"SELECT project_id, entity_id, scene_id, body_kind, mass, lock_rotation,
                      collider_shape, sensor, has_character_controller, extras_json,
                      source_revision
               FROM brain_physics_bodies WHERE scene_id = ? ORDER BY rowid"#,
            scene
        )
        .fetch_all(&self.db.readers)
        .await
        .map_err(|error| db_error(error, "list brain scene physics"))?;
        let mut out = Vec::with_capacity(rows.len());
        for row in rows {
            let Some(entity_id) = row
                .entity_id
                .as_deref()
                .and_then(|s| EntityId::from_str(s).ok())
            else {
                continue;
            };
            let Some(scene_id) = SceneId::from_str(&row.scene_id).ok() else {
                continue;
            };
            let Some(project_id) = ProjectId::from_str(&row.project_id).ok() else {
                continue;
            };
            out.push(PhysicsBodyRecord {
                entity_id,
                project_id,
                scene_id,
                body_kind: row.body_kind,
                mass: row.mass,
                lock_rotation: row.lock_rotation,
                collider_shape: row.collider_shape,
                sensor: row.sensor,
                has_character_controller: row.has_character_controller != 0,
                extras_json: row.extras_json,
                source_revision: row.source_revision,
            });
        }
        Ok(out)
    }

    pub async fn physics_body_by_entity(
        &self,
        entity_id: EntityId,
    ) -> Result<Option<PhysicsBodyRecord>> {
        let id = entity_id.to_string();
        let row = sqlx::query!(
            r#"SELECT project_id, entity_id, scene_id, body_kind, mass, lock_rotation,
                      collider_shape, sensor, has_character_controller, extras_json,
                      source_revision
               FROM brain_physics_bodies WHERE entity_id = ?"#,
            id
        )
        .fetch_optional(&self.db.readers)
        .await
        .map_err(|error| db_error(error, "lookup brain physics body"))?;
        Ok(row.and_then(|r| {
            let scene_id = SceneId::from_str(&r.scene_id).ok()?;
            let project_id = ProjectId::from_str(&r.project_id).ok()?;
            Some(PhysicsBodyRecord {
                entity_id,
                project_id,
                scene_id,
                body_kind: r.body_kind,
                mass: r.mass,
                lock_rotation: r.lock_rotation,
                collider_shape: r.collider_shape,
                sensor: r.sensor,
                has_character_controller: r.has_character_controller != 0,
                extras_json: r.extras_json,
                source_revision: r.source_revision,
            })
        }))
    }
}
