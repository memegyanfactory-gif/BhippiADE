// Tests may panic on purpose: `expect` states a precondition, and a panic here is a failing
// test rather than a crashed app. The workspace-wide `deny` stands in shipping code.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use bhippi_db::Database;
use bhippi_engine::asset::{AssetIndex, AssetKind, AssetRecord, LicenseState};
use bhippi_engine::document::{Entity, SceneDocument};
use bhippi_memory::{hash_content, ProjectBrain, WorldBrain};
use bhippi_types::{AssetId, EntityId, SceneId};
use std::path::PathBuf;

fn test_database_path(label: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "bhippi-brain-{label}-{}.db",
        bhippi_types::SessionId::new()
    ))
}

fn remove_database_files(path: &std::path::Path) {
    let _ = std::fs::remove_file(path);
    let _ = std::fs::remove_file(format!("{}-wal", path.display()));
    let _ = std::fs::remove_file(format!("{}-shm", path.display()));
}

#[tokio::test]
async fn scan_file_persists_and_skips_unchanged() {
    let path = test_database_path("scan");
    let db = Database::connect(&path).await.expect("database must open");

    let project_root = std::env::temp_dir().join("bhippi-test-project");
    let brain = ProjectBrain::new(db.clone(), project_root.clone())
        .await
        .expect("brain must create");

    assert_eq!(brain.project_revision().await, Ok(0));
    assert_eq!(brain.count_symbols().await, Ok(0));

    // First scan — file is new.
    brain
        .scan_file("src/lib.rs", b"fn main() {}")
        .await
        .expect("first scan must succeed");

    let rev1 = brain.project_revision().await.expect("revision must exist");
    assert!(rev1 > 0, "revision should be bumped on first scan");

    // Same content again — no revision bump expected.
    brain
        .scan_file("src/lib.rs", b"fn main() {}")
        .await
        .expect("re-scan must succeed");

    let rev2 = brain.project_revision().await.expect("revision must exist");
    assert_eq!(rev1, rev2, "unchanged file should not bump revision");

    // Different content — revision should bump.
    brain
        .scan_file("src/lib.rs", b"fn main() { println!(\"hi\"); }")
        .await
        .expect("changed scan must succeed");

    let rev3 = brain.project_revision().await.expect("revision must exist");
    assert!(rev3 > rev1, "changed file should bump revision");

    db.close().await;
    remove_database_files(&path);
}

#[tokio::test]
async fn content_hash_is_deterministic() {
    let a = hash_content(b"hello world");
    let b = hash_content(b"hello world");
    assert_eq!(a, b);
    assert!(!a.is_empty());
    assert_eq!(a.len(), 64, "blake3 hex is 64 chars");
}

#[tokio::test]
async fn symbol_lookup_roundtrip() {
    let path = test_database_path("symbol");
    let db = Database::connect(&path).await.expect("database must open");

    let project_root = std::env::temp_dir().join("bhippi-test-symbol-project");
    let brain = ProjectBrain::new(db.clone(), project_root)
        .await
        .expect("brain must create");

    // Insert a file first.
    brain
        .scan_file_with_symbols(
            "src/main.rs",
            b"fn main() {}",
            &[bhippi_memory::SymbolEntry {
                id: bhippi_types::SymbolId::new(),
                kind: "function".to_owned(),
                name: "main".to_owned(),
                qualified_name: "crate::main".to_owned(),
                signature: Some("fn main()".to_owned()),
                body: "fn main() {}".to_owned(),
                start_line: Some(1),
                end_line: Some(1),
                parent_id: None,
            }],
        )
        .await
        .expect("scan with symbols must succeed");

    let count = brain.count_symbols().await.expect("count must work");
    assert_eq!(count, 1);

    let found = brain
        .lookup_symbol("crate::main")
        .await
        .expect("lookup must work");
    assert!(found.is_some(), "symbol should be findable");
    let sym = found.unwrap();
    assert_eq!(sym.kind, "function");
    assert_eq!(sym.name, "main");
    assert_eq!(sym.qualified_name, "crate::main");

    // Re-scan same content — symbol should still be there.
    brain
        .scan_file_with_symbols(
            "src/main.rs",
            b"fn main() {}",
            &[bhippi_memory::SymbolEntry {
                id: sym.id,
                kind: "function".to_owned(),
                name: "main".to_owned(),
                qualified_name: "crate::main".to_owned(),
                signature: Some("fn main()".to_owned()),
                body: "fn main() {}".to_owned(),
                start_line: Some(1),
                end_line: Some(1),
                parent_id: None,
            }],
        )
        .await
        .expect("re-scan must succeed");

    let count = brain.count_symbols().await.expect("count must work");
    assert_eq!(
        count, 1,
        "symbol count should remain 1 after idempotent scan"
    );

    db.close().await;
    remove_database_files(&path);
}

#[tokio::test]
async fn module_registration_is_idempotent() {
    let path = test_database_path("module");
    let db = Database::connect(&path).await.expect("database must open");

    let project_root = std::env::temp_dir().join("bhippi-test-module-project");
    let brain = ProjectBrain::new(db.clone(), project_root)
        .await
        .expect("brain must create");

    brain
        .upsert_module("core")
        .await
        .expect("first upsert must succeed");
    brain
        .upsert_module("core")
        .await
        .expect("second upsert must succeed");
    brain
        .upsert_module("utils")
        .await
        .expect("third upsert must succeed");

    let names = brain.module_names().await.expect("list must work");
    assert_eq!(names.len(), 2);
    assert!(names.contains(&"core".to_owned()));
    assert!(names.contains(&"utils".to_owned()));

    db.close().await;
    remove_database_files(&path);
}

#[tokio::test]
async fn scan_real_files_extracts_symbols_and_rescans() {
    let path = test_database_path("files");
    let db = Database::connect(&path).await.expect("database must open");

    let project_root = std::env::temp_dir().join(format!(
        "bhippi-real-project-{}",
        bhippi_types::SessionId::new()
    ));
    std::fs::create_dir_all(project_root.join("src")).expect("make src dir");

    // Write a Rust source file with functions + struct + methods.
    std::fs::write(
        project_root.join("src/lib.rs"),
        r#"
pub fn greet(name: &str) -> String {
    format!("Hello {name}")
}

struct Counter {
    value: i32,
}

impl Counter {
    fn new() -> Self {
        Self { value: 0 }
    }

    fn increment(&mut self) {
        self.value += 1;
    }
}
"#,
    )
    .expect("write lib.rs");

    let brain = ProjectBrain::new(db.clone(), project_root.clone())
        .await
        .expect("brain must create");

    brain
        .scan_file_auto("src/lib.rs")
        .await
        .expect("index lib.rs");

    let count = brain.count_symbols().await.expect("count symbols");
    assert_eq!(
        count, 5,
        "greet + Counter + new + increment + Counter impl = 5"
    );

    // Lookup a top-level function.
    let found = brain
        .lookup_symbol("src/lib.rs::greet")
        .await
        .expect("lookup greet")
        .expect("greet exists");
    assert_eq!(found.kind, "function");
    assert!(found.signature.is_some());

    // Lookup a method under the impl scope.
    let method = brain
        .lookup_symbol("src/lib.rs::Counter::increment")
        .await
        .expect("lookup increment")
        .expect("increment exists");
    assert_eq!(method.kind, "method");

    // Rescan unchanged content → still 5 symbols (idempotent).
    brain
        .scan_file_auto("src/lib.rs")
        .await
        .expect("re-index lib.rs");
    assert_eq!(
        brain.count_symbols().await.expect("count again"),
        5,
        "idempotent rescan must keep symbol count stable"
    );

    // Modify the file (drop greet) → stale symbol reconciled away.
    std::fs::write(
        project_root.join("src/lib.rs"),
        r#"
struct Counter {
    value: i32,
}

impl Counter {
    fn new() -> Self {
        Self { value: 0 }
    }
}
"#,
    )
    .expect("rewrite lib.rs");
    brain
        .scan_file_auto("src/lib.rs")
        .await
        .expect("re-index modified lib.rs");

    let count = brain.count_symbols().await.expect("count after edit");
    assert_eq!(
        count, 3,
        "Counter + new + impl, greet should be reconciled away"
    );

    let gone = brain
        .lookup_symbol("src/lib.rs::greet")
        .await
        .expect("lookup removed greet");
    assert!(gone.is_none(), "greet should be gone after edit");

    std::fs::remove_dir_all(&project_root).expect("cleanup project");
    db.close().await;
    remove_database_files(&path);
}

#[tokio::test]
async fn embeddings_are_stored_and_search_ranks_exact_over_semantic() {
    let path = test_database_path("embeddings");
    let db = Database::connect(&path).await.expect("database must open");

    let project_root = std::env::temp_dir().join(format!(
        "bhippi-embed-project-{}",
        bhippi_types::SessionId::new()
    ));
    std::fs::create_dir_all(project_root.join("src")).expect("make src dir");

    std::fs::write(
        project_root.join("src/lib.rs"),
        r#"
pub fn greet_user(name: &str) -> String {
    format!("Hello {name}")
}

pub fn update_player_movement(delta: f32) {
    let _ = delta;
}
"#,
    )
    .expect("write lib.rs");

    let brain = ProjectBrain::new(db.clone(), project_root.clone())
        .await
        .expect("brain must create");

    brain
        .scan_file_auto("src/lib.rs")
        .await
        .expect("index lib.rs");

    // Project records the model it was indexed with.
    let model = brain
        .embedding_model_used()
        .await
        .expect("read embedding model")
        .expect("model should be set");
    assert_eq!(model, bhippi_providers::EMBEDDING_MODEL);

    // Every indexed symbol carries a decodeable embedding.
    let greet = brain
        .lookup_symbol("src/lib.rs::greet_user")
        .await
        .expect("lookup greet")
        .expect("greet exists");
    assert!(
        greet.embedding_blob.is_some(),
        "greet should carry an embedding"
    );

    let movement = brain
        .lookup_symbol("src/lib.rs::update_player_movement")
        .await
        .expect("lookup movement")
        .expect("movement exists");
    assert!(
        movement.embedding_blob.is_some(),
        "movement should carry an embedding"
    );

    // Exact name match beats semantic similarity: querying the exact name must
    // surface that symbol first, not a semantically similar one.
    let results = brain
        .search("greet_user", 10)
        .await
        .expect("search by exact name");
    assert!(
        !results.is_empty(),
        "search must return at least one result"
    );
    assert_eq!(
        results[0].name,
        "greet_user",
        "exact match should rank first, got: {:?}",
        results.iter().map(|r| &r.name).collect::<Vec<_>>()
    );

    // Semantic search over a natural-language phrase should rank the player
    // movement symbol above the greeting function.
    let semantic = brain
        .semantic_search("where is player movement handled", 10)
        .await
        .expect("semantic search");
    assert!(!semantic.is_empty(), "semantic search must return results");
    assert_eq!(
        semantic[0].0.name,
        "update_player_movement",
        "movement query should rank the movement symbol first, got: {:?}",
        semantic
            .iter()
            .map(|(s, sim)| (s.name.as_str(), *sim))
            .collect::<Vec<_>>()
    );

    std::fs::remove_dir_all(&project_root).expect("cleanup project");
    db.close().await;
    remove_database_files(&path);
}

#[tokio::test]
async fn reindex_tree_scans_changed_and_removes_gone() {
    let path = test_database_path("reindex");
    let db = Database::connect(&path).await.expect("database must open");

    let project_root = std::env::temp_dir().join(format!(
        "bhippi-reindex-project-{}",
        bhippi_types::SessionId::new()
    ));
    std::fs::create_dir_all(project_root.join("src")).expect("make src dir");
    std::fs::create_dir_all(project_root.join("target")).expect("make excluded dir");

    std::fs::write(
        project_root.join("src/lib.rs"),
        "pub fn keep() -> i32 { 1 }\npub fn drop_me() -> i32 { 2 }\n",
    )
    .expect("write lib.rs");
    // This file lives under an excluded dir and must never be indexed.
    std::fs::write(
        project_root.join("target/generated.rs"),
        "pub fn gen() {}\n",
    )
    .expect("write generated file");

    let brain = ProjectBrain::new(db.clone(), project_root.clone())
        .await
        .expect("brain must create");

    let first = brain.reindex_tree(&[]).await.expect("first reindex");
    assert_eq!(
        first.files_scanned, 1,
        "only lib.rs counts (target excluded)"
    );
    assert_eq!(first.files_changed, 1);
    assert_eq!(first.files_removed, 0);
    assert_eq!(first.symbols_counted, 2, "keep + drop_me");

    // Second run: nothing changed → ignored, no re-index, revision stable.
    let idle = brain.reindex_tree(&[]).await.expect("idle reindex");
    assert_eq!(idle.files_scanned, 1);
    assert_eq!(idle.files_changed, 0);
    assert_eq!(idle.files_removed, 0);
    assert_eq!(
        idle.revision, first.revision,
        "unchanged tree keeps revision"
    );

    // Drop a function, add a file, and let a tracked file disappear.
    std::fs::write(
        project_root.join("src/lib.rs"),
        "pub fn keep() -> i32 { 1 }\npub fn new_fn() -> i32 { 3 }\n",
    )
    .expect("rewrite lib.rs");
    std::fs::write(project_root.join("src/extra.rs"), "pub fn helper() {}\n")
        .expect("write extra.rs");
    std::fs::write(project_root.join("vanishing.rs"), "pub fn ghost() {}\n")
        .expect("write vanishing.rs");
    brain
        .scan_file_auto("vanishing.rs")
        .await
        .expect("index vanishing");

    std::fs::remove_file(project_root.join("vanishing.rs")).expect("remove vanishing.rs");

    let second = brain.reindex_tree(&[]).await.expect("second reindex");
    assert_eq!(second.files_changed, 2, "lib.rs + extra.rs re-indexed");
    assert_eq!(second.files_removed, 1, "vanishing.rs marked stale");
    assert!(
        second.revision > first.revision,
        "tree changed the revision"
    );

    // drop_me is gone (reconciled), new symbols are present.
    assert!(brain
        .lookup_symbol("src/lib.rs::drop_me")
        .await
        .expect("lookup drop_me")
        .is_none());
    assert!(brain
        .lookup_symbol("src/lib.rs::new_fn")
        .await
        .expect("lookup new_fn")
        .is_some());
    assert!(brain
        .lookup_symbol("src/extra.rs::helper")
        .await
        .expect("lookup helper")
        .is_some());
    assert!(
        brain
            .lookup_symbol("vanishing.rs::ghost")
            .await
            .expect("lookup ghost")
            .is_none(),
        "vanished file symbols should be stale/removed"
    );

    std::fs::remove_dir_all(&project_root).expect("cleanup project");
    db.close().await;
    remove_database_files(&path);
}

#[tokio::test]
async fn world_brain_snapshots_scene_graph_and_addresses_entities() {
    let path = test_database_path("world");
    let db = Database::connect(&path).await.expect("database must open");

    let project_root = std::env::temp_dir().join(format!(
        "bhippi-world-project-{}",
        bhippi_types::SessionId::new()
    ));
    let world = WorldBrain::new(db.clone(), &project_root)
        .await
        .expect("world brain must create");

    // Build a nested hierarchy: Environment → Player + Crate.
    let environment = EntityId::new();
    let player = EntityId::new();
    let crate_entity = EntityId::new();
    let mut doc = SceneDocument::empty("level_01");
    doc.entities = vec![
        Entity {
            id: environment,
            name: "Environment".to_owned(),
            parent: None,
            tags: vec![],
            components: Default::default(),
        },
        Entity {
            id: player,
            name: "Player".to_owned(),
            parent: Some(environment),
            tags: vec!["gameplay".to_owned()],
            components: Default::default(),
        },
        Entity {
            id: crate_entity,
            name: "Crate".to_owned(),
            parent: Some(environment),
            tags: vec![],
            components: Default::default(),
        },
    ];

    world
        .index_scene_document("scenes/level_01.bscn.json", &doc, 4)
        .await
        .expect("index scene");

    // Scene is listable and findable by path.
    let scenes = world.project_scenes().await.expect("list scenes");
    assert_eq!(scenes.len(), 1);
    assert_eq!(scenes[0].name, "level_01");
    assert_eq!(scenes[0].kind, "level");
    assert_eq!(scenes[0].entity_count, 3);
    assert_eq!(scenes[0].source_revision, 4);

    let by_path = world
        .scene_by_path("scenes/level_01.bscn.json")
        .await
        .expect("scene by path")
        .expect("scene exists");
    assert_eq!(by_path.scene_id, doc.id);

    // Entities persisted with their hierarchy and stable addresses.
    let entities = world.scene_entities(doc.id).await.expect("scene entities");
    assert_eq!(entities.len(), 3);

    let paths = world.scene_paths(doc.id).await.expect("scene paths");
    let path_map: std::collections::BTreeMap<_, _> = paths.iter().cloned().collect();
    let player_path = path_map
        .get(&player)
        .expect("player has a stable path")
        .to_owned();
    assert_eq!(
        player_path,
        format!("level_01:/Environment/Player#{player}")
    );
    assert!(path_map
        .get(&crate_entity)
        .expect("crate path")
        .contains("Environment/Crate"));

    // find_entity resolves the AI to a stable address by name.
    let found = world
        .find_entity(doc.id, "Crate")
        .await
        .expect("find entity");
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].entity_id, crate_entity);

    // Re-index with the crate removed → per-scene replace keeps only the live set.
    doc.entities.pop();
    world
        .index_scene_document("scenes/level_01.bscn.json", &doc, 5)
        .await
        .expect("re-index scene");
    assert_eq!(
        world
            .scene_entities(doc.id)
            .await
            .expect("entities after")
            .len(),
        2,
        "crate removal should replace the entity set"
    );
    assert_eq!(
        world
            .scene_by_path("scenes/level_01.bscn.json")
            .await
            .expect("scene after")
            .expect("scene exists")
            .entity_count,
        2
    );

    db.close().await;
    remove_database_files(&path);
}

#[tokio::test]
async fn module_cards_are_deterministic_and_update_incrementally() {
    let path = test_database_path("cards");
    let db = Database::connect(&path).await.expect("database must open");

    let project_root = std::env::temp_dir().join(format!(
        "bhippi-card-project-{}",
        bhippi_types::SessionId::new()
    ));
    std::fs::create_dir_all(project_root.join("src")).expect("make src dir");

    std::fs::write(
        project_root.join("src/lib.rs"),
        r#"
pub fn compute_damage(base: i32, armor: i32) -> i32 { base - armor }

pub struct Hero {
    pub name: String,
}

impl Hero {
    fn private_helper(&self) {}
    pub fn level_up(&mut self) {}
}
"#,
    )
    .expect("write lib.rs");

    let brain = ProjectBrain::new(db.clone(), project_root.clone())
        .await
        .expect("brain must create");
    brain
        .scan_file_auto("src/lib.rs")
        .await
        .expect("index lib.rs");

    let card = brain
        .module_card("src/lib.rs")
        .await
        .expect("module card")
        .expect("card exists for indexed file");
    assert_eq!(card.module_name, "src/lib");

    // Deterministic facts: top-level fn is an entry point; methods excluded from both.
    assert_eq!(
        card.entry_points,
        vec!["src/lib.rs::compute_damage".to_owned()]
    );
    assert!(card
        .public_symbols
        .contains(&"src/lib.rs::compute_damage".to_owned()));
    assert!(card.public_symbols.contains(&"src/lib.rs::Hero".to_owned()));
    assert!(
        !card
            .public_symbols
            .iter()
            .any(|s| s.ends_with("::level_up") || s.ends_with("::private_helper")),
        "methods must not be public symbols, got {:?}",
        card.public_symbols
    );
    assert_eq!(
        card.description, None,
        "deterministic card has no AI description"
    );

    let estimate = bhippi_memory::module_card_token_estimate(&card);
    assert!(
        estimate <= 200,
        "module cards must stay compact (<=200 tokens), got {estimate}"
    );

    // Cached: the stored card is returned unchanged on the next call.
    let cached = brain
        .module_card("src/lib.rs")
        .await
        .expect("cached module card")
        .expect("card exists");
    assert_eq!(cached, card, "unchanged module returns the stored card");

    // Incremental update: editing the file changes the symbol set → card recomputed.
    std::fs::write(
        project_root.join("src/lib.rs"),
        r#"
pub fn compute_damage(base: i32, armor: i32) -> i32 { base - armor }
pub fn heal(amount: i32) -> i32 { amount }
"#,
    )
    .expect("rewrite lib.rs");
    brain
        .scan_file_auto("src/lib.rs")
        .await
        .expect("re-index lib.rs");
    let updated = brain
        .module_card("src/lib.rs")
        .await
        .expect("updated module card")
        .expect("card exists");
    assert!(
        updated
            .entry_points
            .contains(&"src/lib.rs::heal".to_owned()),
        "new function should appear as an entry point, got {:?}",
        updated.entry_points
    );
    assert!(
        !updated.public_symbols.iter().any(|s| s.ends_with("::Hero")),
        "deleted struct should be gone from public symbols"
    );

    // All cards are listable.
    let all = brain
        .project_module_cards()
        .await
        .expect("list module cards");
    assert!(all.iter().any(|c| c.module_name == "src/lib"));

    std::fs::remove_dir_all(&project_root).expect("cleanup project");
    db.close().await;
    remove_database_files(&path);
}

#[tokio::test]
async fn world_brain_indexes_asset_graph_and_resolves_reverse_usage() {
    let path = test_database_path("assets");
    let db = Database::connect(&path).await.expect("database must open");

    let project_root = std::env::temp_dir().join(format!(
        "bhippi-world-assets-{}",
        bhippi_types::SessionId::new()
    ));
    std::fs::create_dir_all(&project_root).expect("make project root");
    let world = WorldBrain::new(db.clone(), &project_root)
        .await
        .expect("world brain must create");

    // A scene the assets are (transitively) used by, persisted so reverse usage can
    // resolve its name.
    let scene_id = SceneId::new();
    let mut doc = SceneDocument::empty("level_01");
    doc.id = scene_id;
    world
        .index_scene_document("scenes/level_01.bscn.json", &doc, 3)
        .await
        .expect("index scene");

    // Build an AssetIndex with two assets; the mesh is used by the scene above.
    let mesh_id = AssetId::new();
    let tex_id = AssetId::new();
    let mut index = AssetIndex::default();
    index.assets.insert(
        mesh_id,
        AssetRecord {
            id: mesh_id,
            path_rel: "assets/models/crate.glb".to_owned(),
            kind: AssetKind::Mesh,
            hash: "hash-mesh".to_owned(),
            license: LicenseState::Unknown,
            size_bytes: 10,
            used_by_scenes: vec![scene_id],
        },
    );
    index.assets.insert(
        tex_id,
        AssetRecord {
            id: tex_id,
            path_rel: "assets/textures/wood.png".to_owned(),
            kind: AssetKind::Texture,
            hash: "hash-tex".to_owned(),
            license: LicenseState::Known("CC0".to_owned()),
            size_bytes: 20,
            used_by_scenes: Vec::new(),
        },
    );

    world
        .index_asset_index(&index, 7)
        .await
        .expect("index assets");

    // All assets are listable and findable by id / path / kind.
    let assets = world.project_assets().await.expect("list assets");
    assert_eq!(assets.len(), 2);

    let mesh = world
        .asset_by_id(mesh_id)
        .await
        .expect("asset by id")
        .expect("mesh exists");
    assert_eq!(mesh.kind, "mesh");
    assert_eq!(mesh.source_revision, 7);
    assert_eq!(mesh.rel_path, "assets/models/crate.glb");
    assert_eq!(mesh.license, "unknown");

    let tex = world
        .asset_by_path("assets/textures/wood.png")
        .await
        .expect("asset by path")
        .expect("texture exists");
    assert_eq!(tex.kind, "texture");
    assert_eq!(tex.license, "CC0", "known SPDX must be preserved");

    let meshes = world.assets_by_kind("mesh").await.expect("meshes by kind");
    assert_eq!(meshes.len(), 1);
    assert_eq!(meshes[0].asset_id, mesh_id);

    // Reverse usage: the mesh resolves to the scene that references it.
    let usage = world
        .asset_reverse_usage(mesh_id)
        .await
        .expect("reverse usage");
    assert_eq!(usage, vec!["level_01".to_owned()]);
    assert!(
        world
            .asset_reverse_usage(tex_id)
            .await
            .expect("texture usage")
            .is_empty(),
        "texture is used by no scene"
    );

    // Re-indexing replaces the whole set (rename/removal on disk stays honest).
    index.assets.remove(&tex_id);
    world
        .index_asset_index(&index, 8)
        .await
        .expect("re-index assets");
    assert_eq!(world.project_assets().await.expect("after").len(), 1);
    assert!(
        world
            .asset_by_id(tex_id)
            .await
            .expect("removed texture")
            .is_none(),
        "removed asset should be gone after replace"
    );

    std::fs::remove_dir_all(&project_root).expect("cleanup project");
    db.close().await;
    remove_database_files(&path);
}

#[tokio::test]
async fn world_brain_indexes_physics_bodies_and_colliders() {
    let path = test_database_path("physics");
    let db = Database::connect(&path).await.expect("database must open");

    let project_root = std::env::temp_dir().join(format!(
        "bhippi-world-physics-{}",
        bhippi_types::SessionId::new()
    ));
    std::fs::create_dir_all(&project_root).expect("make project root");
    let world = WorldBrain::new(db.clone(), &project_root)
        .await
        .expect("world brain must create");

    let scene_id = SceneId::new();
    let player = EntityId::new();
    let crate_entity = EntityId::new();
    let sensor_gate = EntityId::new();
    let mut doc = SceneDocument::empty("level_01");
    doc.id = scene_id;
    doc.entities.push(Entity {
        id: player,
        name: "Player".to_owned(),
        parent: None,
        tags: vec![],
        components: serde_json::json!({
            "RigidBody": { "kind": "kinematic", "lock_rotation": true },
            "CharacterController": { "height": 1.8, "radius": 0.35 },
            "Collider": { "shape": { "capsule": [0.35, 1.8] }, "sensor": false },
        })
        .as_object()
        .expect("object map")
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect(),
    });
    doc.entities.push(Entity {
        id: crate_entity,
        name: "Crate".to_owned(),
        parent: None,
        tags: vec![],
        components: serde_json::json!({
            "RigidBody": { "kind": "dynamic", "mass": 70.0, "lock_rotation": true },
            "Collider": { "shape": { "cuboid": [1.0, 1.0, 1.0] }, "sensor": false },
        })
        .as_object()
        .expect("object map")
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect(),
    });
    doc.entities.push(Entity {
        id: sensor_gate,
        name: "SensorGate".to_owned(),
        parent: None,
        tags: vec![],
        components: serde_json::json!({
            "Collider": { "shape": "sphere", "sensor": true },
        })
        .as_object()
        .expect("object map")
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect(),
    });

    world
        .index_scene_document("scenes/level_01.bscn.json", &doc, 4)
        .await
        .expect("index scene");

    // All three physics entities persist and are listable by scene / project.
    let scene_physics = world.scene_physics(scene_id).await.expect("scene physics");
    assert_eq!(scene_physics.len(), 3);
    assert_eq!(
        world
            .project_physics()
            .await
            .expect("project physics")
            .len(),
        3
    );

    // Kinematic controller body: kind + character-controller flag, no mass.
    let player_body = world
        .physics_by_entity(player)
        .await
        .expect("physics by entity")
        .expect("player has a body");
    assert_eq!(player_body.body_kind.as_deref(), Some("kinematic"));
    assert_eq!(player_body.mass, None);
    assert_eq!(player_body.lock_rotation, Some(1));
    assert!(player_body.has_character_controller);
    assert_eq!(player_body.scene_id, scene_id);
    assert_eq!(player_body.source_revision, 4);

    // Dynamic body: mass and collider shape are preserved.
    let crate_body = world
        .physics_by_entity(crate_entity)
        .await
        .expect("physics by entity")
        .expect("crate has a body");
    assert_eq!(crate_body.body_kind.as_deref(), Some("dynamic"));
    assert_eq!(crate_body.mass, Some(70.0));
    assert_eq!(crate_body.lock_rotation, Some(1));
    assert!(crate_body.collider_shape.is_some());
    assert_eq!(crate_body.sensor, Some(0));
    assert!(!crate_body.has_character_controller);

    // Collider-only entity: no body kind, but recorded as a sensor.
    let sensor_body = world
        .physics_by_entity(sensor_gate)
        .await
        .expect("physics by entity")
        .expect("sensor has a collider");
    assert_eq!(sensor_body.body_kind, None);
    assert_eq!(sensor_body.mass, None);
    assert_eq!(sensor_body.sensor, Some(1));

    // A plain entity (no physics component) is not indexed.
    let plain = EntityId::new();
    doc.entities[0].components = Default::default();
    doc.entities[0].id = plain;
    world
        .index_scene_document("scenes/level_01.bscn.json", &doc, 5)
        .await
        .expect("re-index scene");
    assert!(
        world
            .physics_by_entity(plain)
            .await
            .expect("plain lookup")
            .is_none(),
        "entity without a physics component yields no row"
    );
    assert_eq!(
        world.project_physics().await.expect("after re-index").len(),
        2,
        "per-scene replace keeps only the live physics set"
    );

    std::fs::remove_dir_all(&project_root).expect("cleanup project");
    db.close().await;
    remove_database_files(&path);
}
