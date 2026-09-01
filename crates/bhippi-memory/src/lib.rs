//! Persistent semantic memory and entity graph.
//!
//! The Project Brain (`ProjectBrain`) owns high-level indexing and retrieval for a
//! single project: stable symbol IDs, content hashing via blake3, revision tracking,
//! and incremental scan orchestration.  Storage is delegated to `bhippi-db::BrainRepo`.

#![cfg_attr(
    test,
    allow(clippy::expect_used, clippy::unwrap_used),
    doc = "Tests may panic on purpose: `expect` is how a test states its precondition, and a panic there is a failing test rather than a crashed app. The workspace `deny` stands everywhere else."
)]
#![forbid(unsafe_code)]

pub mod parser;

use bhippi_db::{
    AssetRecord as PersistedAsset, BrainRepo, EntityRecord, ModuleCardRecord,
    PhysicsBodyRecord as PersistedBody, SceneRecord, SymbolRecord,
};
use bhippi_engine::asset::{AssetIndex, AssetKind, LicenseState};
use bhippi_engine::query::hierarchy;
use bhippi_engine::SceneDocument;
use bhippi_providers::{EMBEDDING_DIM, EMBEDDING_MODEL};
use bhippi_types::{AssetId, EntityId, FileId, ModuleId, ProjectId, Result, SceneId, SymbolId};
use std::collections::HashSet;
use std::path::PathBuf;

/// Hash raw bytes with blake3 and return the hex digest.
#[must_use]
pub fn hash_content(bytes: &[u8]) -> String {
    blake3::hash(bytes).to_hex().to_string()
}

/// Directories (non-hidden) always skipped by [`ProjectBrain::reindex_tree`]:
/// generated build output and third-party/vendor sources.  Hidden entries (any
/// name starting with `.`, e.g. `.git`, `.env`) are skipped by rule regardless.
#[must_use]
pub fn default_excludes() -> Vec<String> {
    ["target", "node_modules", "dist", "build", "out", "vendor"]
        .iter()
        .map(|s| s.to_string())
        .collect()
}

/// Outcome of an incremental [`ProjectBrain::reindex_tree`] run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IndexResult {
    /// Files visited on disk (skipping excluded/generated paths).
    pub files_scanned: usize,
    /// Files whose content changed and were therefore re-indexed.
    pub files_changed: usize,
    /// Previously-tracked files no longer present on disk, now marked stale.
    pub files_removed: usize,
    /// Live (non-stale) symbols counted after the run.
    pub symbols_counted: u64,
    /// Project source revision after the run.
    pub revision: i64,
}

/// High-level Project Brain API for a single project directory.
#[derive(Clone)]
pub struct ProjectBrain {
    db: bhippi_db::Database,
    project_id: ProjectId,
    project_root: PathBuf,
}

impl ProjectBrain {
    /// Open (or create) the brain for `project_root`.  If the project was already
    /// indexed the existing `ProjectId` is reused; otherwise a fresh one is generated
    /// and persisted.
    pub async fn new(db: bhippi_db::Database, project_root: PathBuf) -> Result<Self> {
        let brain = db.brain();
        let path_str = project_root.to_string_lossy().into_owned();
        let now = chrono::Utc::now();

        let project_id = match brain.project_by_path(&path_str).await? {
            Some(id) => id,
            None => {
                let id = ProjectId::new();
                brain.upsert_project(id, &path_str, &now).await?;
                id
            }
        };

        Ok(Self {
            db,
            project_id,
            project_root,
        })
    }

    #[must_use]
    pub fn project_id(&self) -> ProjectId {
        self.project_id
    }

    #[must_use]
    pub fn project_root(&self) -> &PathBuf {
        &self.project_root
    }

    // ── queries ──────────────────────────────────────────────────────────

    pub async fn project_revision(&self) -> Result<i64> {
        self.brain().project_revision(self.project_id).await
    }

    pub async fn count_symbols(&self) -> Result<u64> {
        self.brain().count_symbols(self.project_id).await
    }

    pub async fn module_names(&self) -> Result<Vec<String>> {
        self.brain().module_names(self.project_id).await
    }

    pub async fn lookup_symbol(&self, qualified_name: &str) -> Result<Option<SymbolRecord>> {
        self.brain()
            .symbol_by_qualified(self.project_id, qualified_name)
            .await
    }

    /// Ranked retrieval over the project's embedded symbols (Phase B5).
    ///
    /// Exact / structural name matches dominate, then a deterministic token-hash
    /// similarity score breaks ties — semantic search is one signal, not the whole
    /// brain (plan SEC. 5).  Only non-stale symbols with an embedding are considered.
    pub async fn search(&self, query: &str, limit: usize) -> Result<Vec<SymbolRecord>> {
        let brain = self.brain();
        let candidates = brain.symbols_with_embeddings(self.project_id).await?;
        let query_lower = query.to_ascii_lowercase();
        let query_emb = bhippi_providers::embed(query);

        let mut scored: Vec<(SymbolRecord, f64)> = Vec::with_capacity(candidates.len());
        for symbol in candidates {
            let mut score = 0.0_f64;
            let name_lower = symbol.name.to_ascii_lowercase();
            let qualified_lower = symbol.qualified_name.to_ascii_lowercase();
            if name_lower == query_lower || qualified_lower == query_lower {
                score += 100.0;
            } else if qualified_lower.ends_with(&format!(".{query_lower}")) {
                score += 50.0;
            } else if qualified_lower.contains(&query_lower) {
                score += 10.0;
            }
            score += symbol_similarity(&query_emb, &symbol) as f64;
            if score > 0.0 {
                scored.push((symbol, score));
            }
        }
        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        scored.truncate(limit.min(scored.len()));
        Ok(scored.into_iter().map(|(symbol, _)| symbol).collect())
    }

    /// Pure semantic similarity search, ranked only by the deterministic embedding.
    /// Prefer [`ProjectBrain::search`] for everyday lookup; this exists for callers
    /// that want raw semantic ranking (e.g. "where is player movement handled?").
    pub async fn semantic_search(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<(SymbolRecord, f32)>> {
        let brain = self.brain();
        let candidates = brain.symbols_with_embeddings(self.project_id).await?;
        let query_emb = bhippi_providers::embed(query);

        let mut scored: Vec<(SymbolRecord, f32)> = Vec::new();
        for symbol in candidates {
            let similarity = symbol_similarity(&query_emb, &symbol);
            scored.push((symbol, similarity));
        }
        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        scored.retain(|(_, sim)| *sim > 0.0);
        scored.truncate(limit.min(scored.len()));
        Ok(scored)
    }

    /// The embedding model/version the project index was most recently built with.
    /// Callers can compare this to `bhippi_providers::EMBEDDING_MODEL` to decide
    /// whether a full re-index is warranted.
    pub async fn embedding_model_used(&self) -> Result<Option<String>> {
        self.brain().embedding_model(self.project_id).await
    }

    // ── mutation ──────────────────────────────────────────────────────────

    /// Register a module name in the brain (idempotent).
    pub async fn upsert_module(&self, name: &str) -> Result<ModuleId> {
        let id = ModuleId::new();
        let now = chrono::Utc::now();
        self.brain()
            .upsert_module(id, self.project_id, name, &now)
            .await?;
        Ok(id)
    }

    /// Scan a single file: hash the content, upsert the file record and (if the
    /// content changed) bump the project revision.  Symbol reconciliation is a
    /// no-op until Phase B4 (structural code indexing) is implemented.
    pub async fn scan_file(&self, rel_path: &str, content: &[u8]) -> Result<()> {
        let brain = self.brain();
        let now = chrono::Utc::now();
        let content_hash = hash_content(content);

        // If the file already has the same hash, nothing to do.  Reuse the file id.
        if let Some(existing) = brain.file_scan(self.project_id, rel_path).await? {
            if existing.content_hash == content_hash {
                return Ok(());
            }
            let file_id = existing.file_id.unwrap_or_else(FileId::new);
            let revision = brain.bump_project_revision(self.project_id, &now).await?;
            brain
                .upsert_file(
                    file_id,
                    self.project_id,
                    rel_path,
                    &content_hash,
                    revision,
                    &now,
                )
                .await?;
            brain
                .reconcile_symbols(self.project_id, file_id, &[], &now)
                .await?;
            return Ok(());
        }

        let revision = brain.bump_project_revision(self.project_id, &now).await?;
        let file_id = FileId::new();
        brain
            .upsert_file(
                file_id,
                self.project_id,
                rel_path,
                &content_hash,
                revision,
                &now,
            )
            .await?;

        // Phase B4 will populate the symbol list here.
        brain
            .reconcile_symbols(self.project_id, file_id, &[], &now)
            .await?;

        Ok(())
    }

    /// Scan a file and record a set of symbols extracted from its content.
    /// `symbols` is the output of the structural indexer (Phase B4).  The
    /// project revision is bumped once, and stale symbols are cleaned up in a
    /// single transaction.
    pub async fn scan_file_with_symbols(
        &self,
        rel_path: &str,
        content: &[u8],
        symbols: &[SymbolEntry],
    ) -> Result<()> {
        let brain = self.brain();
        let now = chrono::Utc::now();
        let content_hash = hash_content(content);

        // Reuse the existing file id so symbols keep pointing at the same file.
        let existing = brain.file_scan(self.project_id, rel_path).await?;
        if let Some(existing) = &existing {
            if existing.content_hash == content_hash {
                return Ok(());
            }
        }
        let file_id = existing.and_then(|e| e.file_id).unwrap_or_else(FileId::new);

        let revision = brain.bump_project_revision(self.project_id, &now).await?;
        brain
            .upsert_file(
                file_id,
                self.project_id,
                rel_path,
                &content_hash,
                revision,
                &now,
            )
            .await?;

        let mut seen = Vec::with_capacity(symbols.len());
        for entry in symbols {
            let content_hash = hash_content(entry.body.as_bytes());

            // Re-embed only changed chunks: reuse a still-live embedding when an
            // identical non-stale symbol already carries one.  Because the model is
            // deterministic, identical text always yields an identical vector, so
            // reuse is exact.
            let (embedding_blob, embedding_dim, embedding_model) = match brain
                .identical_symbol_embedding(
                    self.project_id,
                    file_id,
                    &entry.qualified_name,
                    &content_hash,
                )
                .await?
            {
                Some(blob) => (
                    Some(blob),
                    Some(EMBEDDING_DIM as i64),
                    Some(EMBEDDING_MODEL.to_owned()),
                ),
                None => {
                    let synopsis = format!(
                        "{} {} {} {}",
                        entry.name,
                        entry.qualified_name,
                        entry.signature.as_deref().unwrap_or(""),
                        entry.body
                    );
                    let embedding = bhippi_providers::embed(&synopsis);
                    let blob = bhippi_providers::encode(&embedding);
                    (
                        Some(blob),
                        Some(embedding.dim as i64),
                        Some(embedding.model),
                    )
                }
            };

            let record = SymbolRecord {
                id: entry.id,
                file_id,
                kind: entry.kind.clone(),
                name: entry.name.clone(),
                qualified_name: entry.qualified_name.clone(),
                signature: entry.signature.clone(),
                start_line: entry.start_line,
                end_line: entry.end_line,
                content_hash,
                source_revision: revision,
                stale: false,
                embedding_blob,
                embedding_dim,
                embedding_model,
            };
            brain
                .upsert_symbol(&record, self.project_id, entry.parent_id, &now)
                .await?;
            seen.push(entry.id);
        }

        brain
            .reconcile_symbols(self.project_id, file_id, &seen, &now)
            .await?;
        brain
            .set_embedding_state(self.project_id, EMBEDDING_MODEL, &now)
            .await?;

        Ok(())
    }

    /// Read `rel_path` under the project root, detect its language, extract
    /// symbols with the tree-sitter indexer and persist them.  Skips unknown
    /// file types (no structural index).
    pub async fn scan_file_auto(&self, rel_path: &str) -> Result<()> {
        let path = self.project_root.join(rel_path);
        let bytes = tokio::fs::read(&path)
            .await
            .map_err(|e| bhippi_types::BhippiError::Db {
                reason: format!("read {rel_path} for indexing: {e}"),
                retryable: false,
                hint: Some("Check the file exists and is readable.".to_owned()),
            })?;
        let content = String::from_utf8_lossy(&bytes).into_owned();
        match parser::extract_symbols(rel_path, &content) {
            Some(symbols) => {
                self.scan_file_with_symbols(rel_path, &bytes, &symbols)
                    .await
            }
            None => self.scan_file(rel_path, &bytes).await,
        }
    }

    /// Incrementally re-index the whole project tree (Phase B6, SEC. 9.1).
    ///
    /// Walks the project root, skipping hidden entries (`.git`, `.env`, ...) and
    /// the [`default_excludes`] build/vendor directories.  A file is re-parsed and
    /// re-embedded only when its content hash changed (ignoring unchanged files);
    /// files that were tracked but are now gone are marked stale and their symbols
    /// reconciled away.  The project source revision is bumped once if anything
    /// changed or was removed.  `extra_excludes` adds to (not replaces) the default
    /// set.
    pub async fn reindex_tree(&self, extra_excludes: &[String]) -> Result<IndexResult> {
        let mut exclude: HashSet<String> = default_excludes().into_iter().collect();
        exclude.extend(extra_excludes.iter().cloned());

        let mut files_scanned = 0usize;
        let mut files_changed = 0usize;
        let mut changed = false;
        let mut disk_files = HashSet::new();
        self.walk_tree(
            &exclude,
            &mut disk_files,
            &mut files_scanned,
            &mut files_changed,
            &mut changed,
        )
        .await?;

        // Files tracked by the brain but absent on disk are stale now.
        let tracked = self.brain().file_paths(self.project_id).await?;
        let mut files_removed = 0usize;
        let now = chrono::Utc::now();
        for rel_path in tracked {
            if disk_files.contains(&rel_path) {
                continue;
            }
            if let Some(scan) = self.brain().file_scan(self.project_id, &rel_path).await? {
                if let Some(file_id) = scan.file_id {
                    self.brain()
                        .reconcile_symbols(self.project_id, file_id, &[], &now)
                        .await?;
                }
            }
            self.brain()
                .mark_file_stale(self.project_id, &rel_path, &now)
                .await?;
            files_removed += 1;
            changed = true;
        }

        if changed {
            self.brain()
                .bump_project_revision(self.project_id, &now)
                .await?;
        }

        Ok(IndexResult {
            files_scanned,
            files_changed,
            files_removed,
            symbols_counted: self.count_symbols().await?,
            revision: self.project_revision().await?,
        })
    }

    async fn walk_tree(
        &self,
        exclude: &HashSet<String>,
        disk_files: &mut HashSet<String>,
        files_scanned: &mut usize,
        files_changed: &mut usize,
        changed: &mut bool,
    ) -> Result<()> {
        let mut stack: Vec<PathBuf> = vec![PathBuf::new()];
        while let Some(rel_dir) = stack.pop() {
            let abs_dir = self.project_root.join(&rel_dir);
            let mut entries =
                tokio::fs::read_dir(&abs_dir)
                    .await
                    .map_err(|e| bhippi_types::BhippiError::Io {
                        operation: "read project tree",
                        path: abs_dir.to_string_lossy().into_owned(),
                        reason: e.to_string(),
                        retryable: true,
                        hint: Some("Check the project directory is readable.".to_owned()),
                    })?;
            while let Some(entry) =
                entries
                    .next_entry()
                    .await
                    .map_err(|e| bhippi_types::BhippiError::Io {
                        operation: "read project entry",
                        path: abs_dir.to_string_lossy().into_owned(),
                        reason: e.to_string(),
                        retryable: true,
                        hint: None,
                    })?
            {
                let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
                    continue;
                };
                if name.starts_with('.') {
                    continue;
                }
                let file_type =
                    entry
                        .file_type()
                        .await
                        .map_err(|e| bhippi_types::BhippiError::Io {
                            operation: "stat project entry",
                            path: entry.path().to_string_lossy().into_owned(),
                            reason: e.to_string(),
                            retryable: true,
                            hint: None,
                        })?;
                let rel = if rel_dir.as_os_str().is_empty() {
                    PathBuf::from(&name)
                } else {
                    rel_dir.join(&name)
                };
                if file_type.is_dir() {
                    if !exclude.contains(&name) {
                        stack.push(rel);
                    }
                } else if file_type.is_file() {
                    let rel_str = rel.to_string_lossy().replace('\\', "/");
                    disk_files.insert(rel_str.clone());
                    *files_scanned += 1;
                    let abs = self.project_root.join(&rel);
                    let bytes =
                        tokio::fs::read(&abs)
                            .await
                            .map_err(|e| bhippi_types::BhippiError::Io {
                                operation: "read indexed file",
                                path: abs.to_string_lossy().into_owned(),
                                reason: e.to_string(),
                                retryable: true,
                                hint: None,
                            })?;
                    let content_hash = hash_content(&bytes);
                    let unchanged = matches!(
                        self.brain()
                            .file_scan(self.project_id, &rel_str)
                            .await?,
                        Some(existing) if existing.content_hash == content_hash
                    );
                    if !unchanged {
                        self.scan_file_auto(&rel_str).await?;
                        *files_changed += 1;
                        *changed = true;
                    }
                }
            }
        }
        Ok(())
    }

    // ── module cards (Phase B8, plan SEC. 6) ────────────────────────────

    /// Return the knowledge card for one source file, computing and storing it only
    /// when the file's symbols have changed since it was last built (incremental —
    /// plan SEC. 6: "update cards incrementally").  Returns `None` for a path that
    /// is not currently indexed.  All card facts are deterministic data derived from
    /// the structural index; any AI description is stored separately with provenance.
    pub async fn module_card(&self, rel_path: &str) -> Result<Option<ModuleCard>> {
        let Some(file_id) = self
            .brain()
            .file_scan(self.project_id, rel_path)
            .await?
            .and_then(|s| s.file_id)
        else {
            return Ok(None);
        };
        let symbols = self
            .brain()
            .symbols_for_file(self.project_id, file_id)
            .await?;
        let current_revision = symbols.iter().map(|s| s.source_revision).max().unwrap_or(0);
        let name = module_name(rel_path);

        let stored = self.brain().get_module_card(self.project_id, &name).await?;
        if stored
            .as_ref()
            .is_some_and(|card| card.card_revision == current_revision)
        {
            return Ok(stored.map(ModuleCard::from));
        }

        let card = ModuleCard::from_symbols(name, &symbols, current_revision);
        let record = ModuleCardRecord {
            module_name: card.module_name.clone(),
            entry_points: card.entry_points.clone(),
            public_symbols: card.public_symbols.clone(),
            symbol_count: card.symbol_count as i64,
            description: card.description.clone(),
            description_origin: card.description_origin.clone(),
            card_revision: card.card_revision,
        };
        self.brain()
            .upsert_module_card(self.project_id, &record, &chrono::Utc::now())
            .await?;
        Ok(Some(card))
    }

    /// All stored module cards for the project, in a stable order.
    pub async fn project_module_cards(&self) -> Result<Vec<ModuleCard>> {
        let cards = self.brain().all_module_cards(self.project_id).await?;
        let mut out: Vec<ModuleCard> = cards.into_iter().map(ModuleCard::from).collect();
        out.sort_by(|a, b| a.module_name.cmp(&b.module_name));
        Ok(out)
    }

    fn brain(&self) -> BrainRepo {
        self.db.brain()
    }
}

/// Persistent mirror of the engine's scene graph (World Brain, plan SEC. 7.1,
/// ADR-0024). The engine keeps scenes in memory for live editing; this layer snapshots
/// them so the AI can address world elements (`scene:/Parent/Child#ULID`) across
/// sessions without parsing serialised `.bscn.json` files.
#[derive(Clone)]
pub struct WorldBrain {
    db: bhippi_db::Database,
    project_id: ProjectId,
}

impl WorldBrain {
    /// Open (or create) the world brain for `project_root`. Shares the same
    /// `brain_projects` row as the Project Brain so both views agree on one project id.
    pub async fn new(db: bhippi_db::Database, project_root: &std::path::Path) -> Result<Self> {
        let brain = db.brain();
        let path_str = project_root.to_string_lossy().into_owned();
        let now = chrono::Utc::now();
        let project_id = match brain.project_by_path(&path_str).await? {
            Some(id) => id,
            None => {
                let id = ProjectId::new();
                brain.upsert_project(id, &path_str, &now).await?;
                id
            }
        };
        Ok(Self { db, project_id })
    }

    #[must_use]
    pub fn project_id(&self) -> ProjectId {
        self.project_id
    }

    // ── indexing ─────────────────────────────────────────────────────────

    /// Snapshot one scene document under a project-relative path. Upserts the scene
    /// row and replaces its entities in a single transaction. `source_revision` comes
    /// from the caller so a re-index pass can record which project revision it saw.
    pub async fn index_scene_document(
        &self,
        rel_path: &str,
        doc: &SceneDocument,
        source_revision: i64,
    ) -> Result<()> {
        let brain = self.brain();
        let now = chrono::Utc::now();
        let scene = SceneRecord {
            project_id: self.project_id,
            scene_id: doc.id,
            rel_path: rel_path.to_owned(),
            name: doc.name.clone(),
            kind: scene_kind_name(doc).to_owned(),
            entity_count: i64::try_from(doc.entity_count()).map_err(|error| {
                bhippi_types::BhippiError::Db {
                    reason: format!("scene entity count out of range: {error}"),
                    retryable: false,
                    hint: None,
                }
            })?,
            settings_json: serde_json::to_string(&doc.settings).map_err(|error| {
                bhippi_types::BhippiError::Db {
                    reason: format!("serialize scene settings: {error}"),
                    retryable: false,
                    hint: None,
                }
            })?,
            source_revision,
        };
        brain.upsert_scene(self.project_id, &scene, &now).await?;

        let entries = hierarchy(doc);
        // Build a parent-first, depth-first ordering so the FK on parent_id always
        // sees the parent already inserted.
        let mut records = Vec::with_capacity(entries.len());
        let mut physics = Vec::new();
        for entry in entries {
            let Some(entity) = doc.entity(entry.id) else {
                continue;
            };
            let component_names =
                serde_json::to_string(&entry.component_names).map_err(|error| {
                    bhippi_types::BhippiError::Db {
                        reason: format!("serialize component names: {error}"),
                        retryable: false,
                        hint: None,
                    }
                })?;
            let component_json = serde_json::to_string(&entity.components).map_err(|error| {
                bhippi_types::BhippiError::Db {
                    reason: format!("serialize entity components: {error}"),
                    retryable: false,
                    hint: None,
                }
            })?;
            let tags_json = serde_json::to_string(&entity.tags).map_err(|error| {
                bhippi_types::BhippiError::Db {
                    reason: format!("serialize entity tags: {error}"),
                    retryable: false,
                    hint: None,
                }
            })?;
            records.push(EntityRecord {
                entity_id: entity.id,
                scene_id: doc.id,
                name: entity.name.clone(),
                parent_id: entity.parent,
                tags_json,
                component_names_json: component_names,
                component_json,
                source_revision,
            });
            if let Some(body) =
                physics_body_record(self.project_id, doc.id, entity, source_revision)
            {
                physics.push(body);
            }
        }
        brain
            .replace_scene_entities(self.project_id, doc.id, &records, &now)
            .await?;
        brain
            .replace_scene_physics(self.project_id, doc.id, &physics, &now)
            .await
    }

    // ── queries ──────────────────────────────────────────────────────────

    pub async fn project_scenes(&self) -> Result<Vec<SceneRecord>> {
        self.brain().list_scenes(self.project_id).await
    }

    pub async fn scene_by_path(&self, rel_path: &str) -> Result<Option<SceneRecord>> {
        self.brain().scene_by_path(self.project_id, rel_path).await
    }

    pub async fn scene_entities(&self, scene_id: SceneId) -> Result<Vec<EntityRecord>> {
        self.brain().scene_entities(scene_id).await
    }

    /// Everything under one scene as `scene:/root/...` stable paths (the address shape
    /// the AI uses to reference world elements, plan SEC. 9.1 / 7.1). Requires the
    /// scene row for its name; the `_project_id` exists so callers can scope lookups.
    pub async fn scene_paths(&self, scene_id: SceneId) -> Result<Vec<(EntityId, String)>> {
        let brain = self.brain();
        let Some(scene) = brain.scene_by_id(self.project_id, scene_id).await? else {
            return Ok(Vec::new());
        };
        let entities = brain.scene_entities(scene_id).await?;
        let name = scene.name;
        Ok(stable_paths(&name, &entities, self.project_id))
    }

    pub async fn find_entity(&self, scene_id: SceneId, name: &str) -> Result<Vec<EntityRecord>> {
        Ok(self
            .brain()
            .scene_entities(scene_id)
            .await?
            .into_iter()
            .filter(|entity| entity.name == name)
            .collect())
    }

    pub async fn entity_by_id(&self, entity_id: EntityId) -> Result<Option<EntityRecord>> {
        self.brain().entity_by_id(entity_id).await
    }

    pub async fn remove_scene(&self, scene_id: SceneId) -> Result<()> {
        self.brain().remove_scene(scene_id).await
    }

    // ── asset graph (SEC. 7.2, ADR-0025) ──────────────────────────────────

    /// Persist the project's whole asset index into the World Brain, replacing the
    /// previous snapshot. `source_revision` comes from the caller so a re-index pass
    /// can record which project revision it saw. Reverse usage ("what uses this
    /// asset?") is carried over from the engine's `record.used_by_scenes`.
    pub async fn index_asset_index(&self, index: &AssetIndex, source_revision: i64) -> Result<()> {
        let now = chrono::Utc::now();
        let records: Vec<PersistedAsset> = index
            .assets
            .values()
            .map(|record| persisted_asset(self.project_id, record, source_revision))
            .collect();
        self.brain()
            .replace_project_assets(self.project_id, &records, &now)
            .await
    }

    pub async fn project_assets(&self) -> Result<Vec<PersistedAsset>> {
        self.brain().assets_by_project(self.project_id).await
    }

    pub async fn asset_by_id(&self, asset_id: AssetId) -> Result<Option<PersistedAsset>> {
        self.brain().asset_by_id(self.project_id, asset_id).await
    }

    pub async fn asset_by_path(&self, rel_path: &str) -> Result<Option<PersistedAsset>> {
        self.brain().asset_by_path(self.project_id, rel_path).await
    }

    pub async fn assets_by_kind(&self, kind: &str) -> Result<Vec<PersistedAsset>> {
        self.brain().assets_by_kind(self.project_id, kind).await
    }

    /// Scenes that reference an asset, resolved to their names — the reverse-usage
    /// answer for "what uses this material/texture/...?". Returns an empty list when
    /// the asset is unknown.
    pub async fn asset_reverse_usage(&self, asset_id: AssetId) -> Result<Vec<String>> {
        let brain = self.brain();
        let Some(asset) = brain.asset_by_id(self.project_id, asset_id).await? else {
            return Ok(Vec::new());
        };
        let scene_ids: Vec<SceneId> =
            serde_json::from_str(&asset.used_by_scenes_json).unwrap_or_default();
        let mut names = Vec::with_capacity(scene_ids.len());
        for scene_id in scene_ids {
            if let Some(scene) = brain.scene_by_id(self.project_id, scene_id).await? {
                names.push(scene.name);
            }
        }
        names.sort();
        names.dedup();
        Ok(names)
    }

    // ── physics graph (SEC. 7.3, ADR-0026) ──────────────────────────────

    /// Every rigid body / collider recorded across the project's scenes.
    pub async fn project_physics(&self) -> Result<Vec<PersistedBody>> {
        self.brain()
            .physics_bodies_by_project(self.project_id)
            .await
    }

    /// Rigid bodies / colliders belonging to one scene.
    pub async fn scene_physics(&self, scene_id: SceneId) -> Result<Vec<PersistedBody>> {
        self.brain().physics_bodies_by_scene(scene_id).await
    }

    /// The physics record for one entity, if it carries any physics component.
    pub async fn physics_by_entity(&self, entity_id: EntityId) -> Result<Option<PersistedBody>> {
        self.brain().physics_body_by_entity(entity_id).await
    }

    fn brain(&self) -> BrainRepo {
        self.db.brain()
    }
}

fn persisted_asset(
    project_id: ProjectId,
    record: &bhippi_engine::asset::AssetRecord,
    source_revision: i64,
) -> PersistedAsset {
    let used_by_scenes_json =
        serde_json::to_string(&record.used_by_scenes).unwrap_or_else(|_| "[]".to_owned());
    let license = match &record.license {
        LicenseState::Unknown => "unknown".to_owned(),
        LicenseState::Known(spdx) => spdx.clone(),
    };
    PersistedAsset {
        asset_id: record.id,
        project_id,
        rel_path: record.path_rel.clone(),
        kind: asset_kind_name(record.kind).to_owned(),
        hash: record.hash.clone(),
        license,
        size_bytes: i64::try_from(record.size_bytes).unwrap_or(i64::MAX),
        used_by_scenes_json,
        source_revision,
    }
}

/// Derive a persisted physics body/collider record for an entity, or `None` when the
/// entity carries no physics component. Purely a projection of the entity's authored
/// `RigidBody` / `Collider` / `CharacterController` components (ADR-0026, SEC. 7.3).
fn physics_body_record(
    project_id: ProjectId,
    scene_id: SceneId,
    entity: &bhippi_engine::document::Entity,
    source_revision: i64,
) -> Option<PersistedBody> {
    let mut body_kind = None;
    let mut mass = None;
    let mut lock_rotation = None;
    let mut collider_shape = None;
    let mut sensor = None;
    let mut has_character_controller = false;
    let mut has_physics = false;

    if let Some(rigid) = entity.components.get("RigidBody") {
        has_physics = true;
        if let Some(kind) = rigid.get("kind").and_then(serde_json::Value::as_str) {
            body_kind = Some(kind.to_owned());
        }
        mass = rigid.get("mass").and_then(serde_json::Value::as_f64);
        lock_rotation = rigid
            .get("lock_rotation")
            .and_then(serde_json::Value::as_bool)
            .map(|value| if value { 1 } else { 0 });
    }
    if let Some(collider) = entity.components.get("Collider") {
        has_physics = true;
        collider_shape = collider
            .get("shape")
            .and_then(|shape| serde_json::to_string(shape).ok());
        sensor = collider
            .get("sensor")
            .and_then(serde_json::Value::as_bool)
            .map(|value| if value { 1 } else { 0 });
    }
    if entity.components.contains_key("CharacterController") {
        has_physics = true;
        has_character_controller = true;
    }

    if !has_physics {
        return None;
    }

    Some(PersistedBody {
        entity_id: entity.id,
        project_id,
        scene_id,
        body_kind,
        mass,
        lock_rotation,
        collider_shape,
        sensor,
        has_character_controller,
        extras_json: "{}".to_owned(),
        source_revision,
    })
}

fn asset_kind_name(kind: AssetKind) -> &'static str {
    match kind {
        AssetKind::Mesh => "mesh",
        AssetKind::Skeleton => "skeleton",
        AssetKind::Texture => "texture",
        AssetKind::Material => "material",
        AssetKind::Audio => "audio",
        AssetKind::Animation => "animation",
        AssetKind::Scene => "scene",
        AssetKind::Script => "script",
        AssetKind::Prefab => "prefab",
        AssetKind::Ui => "ui",
        AssetKind::Font => "font",
        AssetKind::Shader => "shader",
        AssetKind::Other => "other",
    }
}

fn scene_kind_name(doc: &SceneDocument) -> &'static str {
    use bhippi_engine::document::SceneKind;
    match doc.settings.kind {
        SceneKind::Main => "main",
        SceneKind::Level => "level",
        SceneKind::Hud => "hud",
        SceneKind::Empty => "empty",
    }
}

/// Deterministic `scene:/root/.../name#ULID` addresses for every entity, using the
/// persisted scene name and the flat row set (parent chains are resolved in memory).
#[must_use]
fn stable_paths(
    scene_name: &str,
    entities: &[EntityRecord],
    _project_id: ProjectId,
) -> Vec<(EntityId, String)> {
    let by_id: std::collections::BTreeMap<EntityId, &EntityRecord> = entities
        .iter()
        .map(|entity| (entity.entity_id, entity))
        .collect();
    let mut out = Vec::with_capacity(entities.len());
    for entity in entities {
        let mut chain = vec![entity.name.clone()];
        let mut current = entity.parent_id;
        while let Some(parent) = current {
            let Some(parent_row) = by_id.get(&parent) else {
                break;
            };
            chain.push(parent_row.name.clone());
            current = parent_row.parent_id;
            if chain.len() > entities.len() {
                break;
            }
        }
        chain.reverse();
        out.push((
            entity.entity_id,
            format!("{scene_name}:/{}#{}", chain.join("/"), entity.entity_id),
        ));
    }
    out
}

/// Cosine similarity between a query embedding and a stored symbol embedding.
/// Returns `0.0` when the symbol has no decodable embedding or the query is empty.
fn symbol_similarity(query: &bhippi_providers::Embedding, symbol: &SymbolRecord) -> f32 {
    let Some(blob) = &symbol.embedding_blob else {
        return 0.0;
    };
    let Some(embedding) = bhippi_providers::decode(blob) else {
        return 0.0;
    };
    bhippi_providers::cosine(&query.values, &embedding.values).unwrap_or(0.0)
}

/// Compact knowledge card for one source module (Phase B8, plan SEC. 6).
///
/// Facts (`public_symbols`, `entry_points`, `symbol_count`) are derived deterministically
/// from the structural index and are always trustworthy.  Any AI-generated `description`
/// is stored separately and carries a `description_origin` provenance marker so it can
/// never be mistaken for a hard fact (plan SEC. 6: "store descriptions separately from
/// hard facts", "mark generated claims with provenance").
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModuleCard {
    /// Module identity, e.g. `src/lib` for `src/lib.rs`.
    pub module_name: String,
    /// Qualified names of top-level functions (callable entry points).
    pub entry_points: Vec<String>,
    /// Qualified names of top-level items (functions, types, traits, consts...).
    pub public_symbols: Vec<String>,
    /// Total live symbols in the module.
    pub symbol_count: usize,
    /// Optional AI-generated description; `None` means "not generated yet".
    pub description: Option<String>,
    /// Provenance of `description`, e.g. which model generated it.
    pub description_origin: Option<String>,
    /// Max symbol source_revision this card was built at.
    pub card_revision: i64,
}

impl ModuleCard {
    fn from_symbols(module_name: String, symbols: &[SymbolRecord], card_revision: i64) -> Self {
        let mut entry_points: Vec<String> = symbols
            .iter()
            .filter(|s| s.kind == "function")
            .map(|s| s.qualified_name.clone())
            .collect();
        let mut public_symbols: Vec<String> = symbols
            .iter()
            .filter(|s| s.kind != "method")
            .map(|s| s.qualified_name.clone())
            .collect();
        entry_points.sort();
        public_symbols.sort();
        Self {
            module_name,
            symbol_count: symbols.len(),
            entry_points,
            public_symbols,
            description: None,
            description_origin: None,
            card_revision,
        }
    }
}

impl From<ModuleCardRecord> for ModuleCard {
    fn from(record: ModuleCardRecord) -> Self {
        Self {
            module_name: record.module_name,
            entry_points: record.entry_points,
            public_symbols: record.public_symbols,
            symbol_count: record.symbol_count as usize,
            description: record.description,
            description_origin: record.description_origin,
            card_revision: record.card_revision,
        }
    }
}

/// Derive a module identity from a rel path: the path minus its extension
/// (`src/lib.rs` → `src/lib`), so files in different directories never collide.
fn module_name(rel_path: &str) -> String {
    let path = PathBuf::from(rel_path);
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| rel_path.to_owned());
    match path.parent().filter(|p| !p.as_os_str().is_empty()) {
        Some(parent) => format!("{}/{}", parent.to_string_lossy(), stem),
        None => stem,
    }
}

/// Rough token estimate for a [`ModuleCard`] (plan SEC. 6 target: ~50–200 tokens).
/// Assumes ~4 characters per token.
#[must_use]
pub fn module_card_token_estimate(card: &ModuleCard) -> usize {
    let mut chars = card.module_name.len() + 8;
    for name in &card.public_symbols {
        chars += name.len() + 2;
    }
    for name in &card.entry_points {
        chars += name.len() + 2;
    }
    chars.div_ceil(4)
}

/// Input to `scan_file_with_symbols`.  The structural indexer (Phase B4)
/// produces a `Vec<SymbolEntry>` per file.
pub struct SymbolEntry {
    pub id: SymbolId,
    pub kind: String,
    pub name: String,
    pub qualified_name: String,
    pub signature: Option<String>,
    pub body: String,
    pub start_line: Option<i64>,
    pub end_line: Option<i64>,
    pub parent_id: Option<SymbolId>,
}
