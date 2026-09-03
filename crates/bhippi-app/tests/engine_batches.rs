//! ENG-119 — golden transcripts for the AI ↔ engine bridge.
//!
//! Each test is a request a user would actually make, expressed exactly as the model is
//! told to express it in `prompts/chat-engine.md`, driven through the same session store
//! the chat bridge uses. They exist to keep three promises honest: a batch is one undo, a
//! rejected batch writes nothing, and a rejection teaches the model the real schema.

// Tests may panic on purpose: `expect` states a precondition, and a panic here is a failing
// test rather than a crashed app. The workspace-wide `deny` stands in shipping code.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use bhippi_app::engine::session::{BatchRequest, EngineSessions};
use bhippi_engine::scaffold;
use bhippi_types::{EngineActor, EntityId};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

const LEVEL: &str = "assets/scenes/level_01.bscn.json";

fn temp_game(label: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("bhippi-batch-{label}-{}", EntityId::new()));
    std::fs::create_dir_all(dir.join("assets/scenes")).expect("scene folder");
    std::fs::write(
        dir.join(LEVEL),
        scaffold::starter_scene().dump().expect("dump"),
    )
    .expect("write scene");
    dir
}

/// The app's own resolver, so these transcripts exercise the real path rather than a copy
/// of it that could drift away from what the chat bridge actually does.
use bhippi_app::engine::resolve_batch_step as resolve;

fn run(
    sessions: &mut EngineSessions,
    game_dir: &Path,
    label: &str,
    actions: &[Value],
) -> bhippi_app::engine::session::EngineBatchResult {
    sessions
        .apply_batch(
            BatchRequest {
                game_dir,
                rel_path: LEVEL,
                label,
                actions,
                actor: EngineActor::Agent,
                autosave: true,
                owner: None,
                base_revision: None,
            },
            resolve,
        )
        .expect("the batch call itself must not error")
        .result
}

fn named(sessions: &EngineSessions, game_dir: &Path, name: &str) -> bool {
    sessions
        .document(game_dir, LEVEL)
        .expect("open")
        .entities
        .iter()
        .any(|entity| entity.name == name)
}

/// "Build me a loading dock" — the headline case. Many actions, one undo.
#[test]
fn a_whole_build_request_is_one_batch_and_one_undo() {
    let game_dir = temp_game("dock");
    let mut sessions = EngineSessions::new();
    let before = sessions.open(&game_dir, LEVEL).expect("open").entity_count;

    let result = run(
        &mut sessions,
        &game_dir,
        "build the loading dock",
        &[
            json!({ "kind": "spawn", "template": "plane", "name": "DockFloor", "at": [0, 0, 0] }),
            json!({ "kind": "spawn", "template": "cube", "name": "Crate A", "at": [2, 0.5, 1] }),
            json!({ "kind": "spawn", "template": "cube", "name": "Crate B", "at": [3.2, 0.5, 1] }),
            // References a crate this same batch created two actions ago.
            json!({ "kind": "group_entities", "entities": ["Crate A", "Crate B"], "name": "Crates" }),
            json!({ "kind": "align_entities", "entities": ["Crate A", "Crate B"], "axis": "y", "mode": "min" }),
            json!({ "kind": "set_weather", "weather": "overcast" }),
        ],
    );

    assert!(result.applied, "{}", result.summary());
    assert_eq!(result.outcomes.len(), 6);
    assert!(result.outcomes.iter().all(|outcome| outcome.ok));

    let state = &result.state;
    // 3 spawns + the group node the batch created.
    assert_eq!(state.entity_count, before + 4);
    assert_eq!(state.settings.weather.as_deref(), Some("overcast"));
    assert!(named(&sessions, &game_dir, "Crates"));
    assert_eq!(
        state.undo_label.as_deref(),
        Some("build the loading dock"),
        "the Undo button names the change the user asked for"
    );

    // One Ctrl+Z removes the whole dock.
    let after_undo = sessions.undo(&game_dir, LEVEL).expect("undo");
    assert_eq!(after_undo.entity_count, before);
    assert!(!named(&sessions, &game_dir, "Crates"));
    assert!(!named(&sessions, &game_dir, "DockFloor"));
    assert!(
        !after_undo.can_undo,
        "the batch was a single entry on the stack"
    );

    let _ = std::fs::remove_dir_all(&game_dir);
}

/// A batch that fails halfway writes nothing at all — not the actions before the failure.
#[test]
fn a_rejected_batch_leaves_the_scene_exactly_as_it_was() {
    let game_dir = temp_game("reject");
    let mut sessions = EngineSessions::new();
    let before = sessions.open(&game_dir, LEVEL).expect("open").entity_count;

    let result = run(
        &mut sessions,
        &game_dir,
        "half-valid build",
        &[
            json!({ "kind": "spawn", "template": "cube", "name": "GoodCrate" }),
            json!({ "kind": "spawn", "template": "flying-saucer", "name": "Nope" }),
            json!({ "kind": "spawn", "template": "cube", "name": "NeverReached" }),
        ],
    );

    assert!(!result.applied);
    let state = sessions.open(&game_dir, LEVEL).expect("open");
    assert_eq!(state.entity_count, before, "nothing was written");
    assert!(!named(&sessions, &game_dir, "GoodCrate"));
    assert!(
        !state.dirty,
        "a rejected batch does not even dirty the scene"
    );
    assert!(!state.can_undo, "and leaves no undo entry");

    // The envelope names the offending index and stops there.
    assert_eq!(result.outcomes.len(), 2, "it stops at the first failure");
    assert!(result.outcomes[0].ok);
    assert_eq!(result.outcomes[1].index, 1);
    assert!(!result.outcomes[1].ok);
    assert!(result.outcomes[1].message.contains("flying-saucer"));
    assert!(
        result.outcomes[1]
            .hint
            .as_deref()
            .is_some_and(|hint| hint.contains("cube")),
        "the hint offers the real palette"
    );

    let _ = std::fs::remove_dir_all(&game_dir);
}

/// A bad component value comes back with that component's real schema attached, which is
/// what makes the repair round work without a human.
#[test]
fn a_bad_component_value_returns_the_real_schema() {
    let game_dir = temp_game("schema");
    let mut sessions = EngineSessions::new();
    sessions.open(&game_dir, LEVEL).expect("open");

    let result = run(
        &mut sessions,
        &game_dir,
        "make the player a physics body",
        &[json!({
            "kind": "add_component",
            "entity": "Player",
            "component": "RigidBody",
            "value": { "kind": "bouncy", "mass": 70.0 }
        })],
    );

    assert!(!result.applied);
    let outcome = &result.outcomes[0];
    assert!(!outcome.ok);
    let excerpt = outcome
        .schema_excerpt
        .as_deref()
        .expect("a component failure carries its schema");
    assert!(excerpt.contains("RigidBody"));
    assert!(excerpt.contains("static|dynamic|kinematic"));
    assert!(excerpt.contains("lock_rotation"));

    let prompt = bhippi_app::engine::bridge::continuation_prompt(&[], &[result])
        .expect("a rejection owes a repair round");
    assert!(prompt.contains("REJECTED"));
    assert!(prompt.contains("static|dynamic|kinematic"));

    let _ = std::fs::remove_dir_all(&game_dir);
}

/// "The lamp should point at the player, and dim it" — the read-free verbs.
#[test]
fn aim_and_adjust_without_the_model_computing_anything() {
    let game_dir = temp_game("aim");
    let mut sessions = EngineSessions::new();
    sessions.open(&game_dir, LEVEL).expect("open");

    let result = run(
        &mut sessions,
        &game_dir,
        "aim the sun at the player and dim it",
        &[
            json!({ "kind": "look_at", "entity": "Sun", "target": "Player" }),
            json!({ "kind": "set_component_property", "entity": "Sun", "component": "Light", "path": "intensity", "value": 0.4 }),
            json!({ "kind": "translate", "entity": "Player", "by": [0, 0, -2] }),
        ],
    );
    assert!(result.applied, "{}", result.summary());

    let doc = sessions.document(&game_dir, LEVEL).expect("open");
    let sun = doc
        .entities
        .iter()
        .find(|entity| entity.name == "Sun")
        .expect("Sun");
    assert_eq!(sun.components["Light"]["intensity"], 0.4);
    // The quaternion is finite and normalised — the engine did the maths, not the model.
    let rot = &sun.components["Transform"]["rot"];
    let length: f64 = (0..4)
        .map(|index| {
            let value = rot[index].as_f64().unwrap_or(0.0);
            value * value
        })
        .sum::<f64>()
        .sqrt();
    assert!((length - 1.0).abs() < 1e-4, "unit quaternion, got {length}");

    let _ = std::fs::remove_dir_all(&game_dir);
}

/// An empty batch is refused rather than journaled as a no-op change.
#[test]
fn an_empty_batch_is_refused() {
    let game_dir = temp_game("empty");
    let mut sessions = EngineSessions::new();
    let error = sessions
        .apply_batch(
            BatchRequest {
                game_dir: &game_dir,
                rel_path: LEVEL,
                label: "nothing",
                actions: &[],
                actor: EngineActor::Agent,
                autosave: true,
                owner: None,
                base_revision: None,
            },
            resolve,
        )
        .expect_err("an empty batch is a mistake, not a change");
    assert!(error.hint.is_some());
    let _ = std::fs::remove_dir_all(&game_dir);
}

/// A successful batch journals as a single transaction carrying every op and its inverse,
/// so it stays replayable and reversible after a restart.
#[test]
fn a_batch_journals_once_with_its_whole_inverse() {
    let game_dir = temp_game("journal");
    let mut sessions = EngineSessions::new();
    sessions.open(&game_dir, LEVEL).expect("open");

    let actions = [
        json!({ "kind": "spawn", "template": "cube", "name": "One" }),
        json!({ "kind": "spawn", "template": "cube", "name": "Two" }),
    ];
    let applied = sessions
        .apply_batch(
            BatchRequest {
                game_dir: &game_dir,
                rel_path: LEVEL,
                label: "two crates",
                actions: &actions,
                actor: EngineActor::Agent,
                autosave: true,
                owner: None,
                base_revision: None,
            },
            resolve,
        )
        .expect("applies");

    assert!(applied.result.applied);
    let facts = applied.journal.expect("an applied batch is journalable");
    assert_eq!(facts.op_count, 2, "one row for the whole batch");
    assert_eq!(facts.actor, "agent");
    assert!(facts.label.contains("two crates"));
    let ops: Vec<Value> = serde_json::from_str(&facts.ops_json).expect("ops round-trip");
    assert_eq!(ops.len(), 2);
    let inverse: Vec<Value> = serde_json::from_str(&facts.inverse_json).expect("inverse");
    assert_eq!(
        inverse.len(),
        2,
        "both spawns are reversible from the journal"
    );

    let _ = std::fs::remove_dir_all(&game_dir);
}

/// The Phase 2 promise, end to end: "make a wet concrete material and put it on the floor"
/// is **one** change — the file is written, the mesh is pointed at it, and one Ctrl+Z undoes
/// both. This is the case that justifies content actions riding inside a scene transaction.
#[test]
fn creating_a_material_and_assigning_it_is_one_undoable_change() {
    let game_dir = temp_game("material");
    let mut sessions = EngineSessions::new();
    sessions.open(&game_dir, LEVEL).expect("open");

    let result = run(
        &mut sessions,
        &game_dir,
        "wet concrete floor",
        &[
            json!({
                "kind": "create_material",
                "name": "Wet Concrete",
                "params": { "roughness": 0.15, "metallic": 0.0 }
            }),
            json!({
                "kind": "set_material",
                "entity": "Floor",
                "material": "assets/materials/wet_concrete.mat.json"
            }),
        ],
    );
    assert!(result.applied, "{}", result.summary());

    let material_path = game_dir.join("assets/materials/wet_concrete.mat.json");
    assert!(material_path.is_file(), "the material file was written");
    let text = std::fs::read_to_string(&material_path).expect("read");
    let material = bhippi_engine::material::MaterialDocument::parse(&text)
        .expect("it validates as a document");
    assert_eq!(material.params.roughness, 0.15);

    let floor = sessions
        .document(&game_dir, LEVEL)
        .expect("open")
        .entities
        .iter()
        .find(|entity| entity.name == "Floor")
        .expect("Floor")
        .clone();
    assert_eq!(
        floor.components["MeshRenderer"]["materials"][0],
        "assets/materials/wet_concrete.mat.json"
    );

    // One undo takes back the assignment AND deletes the generated file.
    sessions.undo(&game_dir, LEVEL).expect("undo");
    assert!(
        !material_path.is_file(),
        "undoing the change removes the asset it generated"
    );
    assert!(
        !game_dir
            .join("assets/materials/wet_concrete.mat.json.meta.json")
            .is_file(),
        "and its sidecar"
    );

    // Redo puts both back.
    sessions.redo(&game_dir, LEVEL).expect("redo");
    assert!(material_path.is_file(), "redo rewrites the asset");
    let floor = sessions
        .document(&game_dir, LEVEL)
        .expect("open")
        .entities
        .iter()
        .find(|entity| entity.name == "Floor")
        .expect("Floor")
        .clone();
    assert_eq!(
        floor.components["MeshRenderer"]["materials"][0],
        "assets/materials/wet_concrete.mat.json"
    );

    let _ = std::fs::remove_dir_all(&game_dir);
}

/// A batch that writes a file and then fails must leave no file behind. A half-created
/// asset folder is exactly the debris that makes a generated project untrustworthy.
#[test]
fn a_failed_batch_rolls_back_the_files_it_had_already_written() {
    let game_dir = temp_game("rollback");
    let mut sessions = EngineSessions::new();
    sessions.open(&game_dir, LEVEL).expect("open");

    let result = run(
        &mut sessions,
        &game_dir,
        "material then nonsense",
        &[
            json!({ "kind": "create_material", "name": "Ghost" }),
            json!({ "kind": "spawn", "template": "flying-saucer" }),
        ],
    );

    assert!(!result.applied);
    assert!(
        !game_dir.join("assets/materials/ghost.mat.json").exists(),
        "the file written before the failure was rolled back"
    );
    assert!(
        !game_dir
            .join("assets/materials/ghost.mat.json.meta.json")
            .exists(),
        "including its sidecar"
    );
    assert!(!sessions.open(&game_dir, LEVEL).expect("open").can_undo);

    let _ = std::fs::remove_dir_all(&game_dir);
}

/// Capturing a prop as a prefab and stamping it is the "objects, used properly" half.
#[test]
fn a_prefab_can_be_captured_and_its_document_validates() {
    let game_dir = temp_game("prefab");
    let mut sessions = EngineSessions::new();
    sessions.open(&game_dir, LEVEL).expect("open");

    let result = run(
        &mut sessions,
        &game_dir,
        "capture the player as a prefab",
        &[json!({ "kind": "create_prefab", "name": "Hero", "entity": "Player" })],
    );
    assert!(result.applied, "{}", result.summary());

    let path = game_dir.join("assets/prefabs/hero.prefab.json");
    assert!(path.is_file());
    let text = std::fs::read_to_string(&path).expect("read");
    let prefab = bhippi_engine::prefab::PrefabDocument::parse(&text).expect("validates");
    assert_eq!(prefab.name, "Hero");
    assert_eq!(prefab.roots().len(), 1);

    let _ = std::fs::remove_dir_all(&game_dir);
}

/// A generated asset knows its licence, so it never blocks a Release build for a reason
/// nobody can act on (INV-074).
#[test]
fn generated_assets_carry_a_licence_so_release_builds_are_not_blocked_by_them() {
    let game_dir = temp_game("licence");
    let mut sessions = EngineSessions::new();
    sessions.open(&game_dir, LEVEL).expect("open");

    run(
        &mut sessions,
        &game_dir,
        "a material",
        &[json!({ "kind": "create_material", "name": "Brass" })],
    );

    let index = bhippi_engine::asset::AssetIndex::scan(&game_dir).expect("scan");
    let brass = index
        .by_path("assets/materials/brass.mat.json")
        .expect("the generated material is indexed");
    assert_ne!(
        brass.license,
        bhippi_engine::asset::LicenseState::Unknown,
        "a generated asset states where it came from"
    );

    // And nothing the batch generated appears among the Release blockers. (The fixture
    // scene here is hand-written without a sidecar; a real scaffolded project writes them,
    // which `a_scaffolded_game_can_produce_a_release_build` covers.)
    let report = bhippi_engine::gates::check_assets(&index, &[], true);
    assert!(
        !report
            .blockers()
            .iter()
            .any(|finding| finding.message.contains("brass")),
        "a generated material must never be a Release blocker: {:?}",
        report.findings
    );

    let _ = std::fs::remove_dir_all(&game_dir);
}

/// A malformed content payload is reported as such rather than falling through and being
/// misdiagnosed as an unknown scene action.
#[test]
fn a_content_action_missing_its_fields_says_so() {
    let game_dir = temp_game("malformed");
    let mut sessions = EngineSessions::new();
    sessions.open(&game_dir, LEVEL).expect("open");

    let result = run(
        &mut sessions,
        &game_dir,
        "bad material",
        &[json!({ "kind": "create_material" })],
    );
    assert!(!result.applied);
    let outcome = &result.outcomes[0];
    assert!(outcome.message.contains("create_material"));
    assert!(outcome.hint.is_some());

    let _ = std::fs::remove_dir_all(&game_dir);
}

/// A brand-new project must be able to produce a Release build. INV-074 blocks on
/// `license = unknown`, and until now the scaffold wrote its own scenes, material and
/// shader with no sidecar — so a fresh New Game was blocked by its own starter content.
#[test]
fn a_scaffolded_game_can_produce_a_release_build() {
    let root = std::env::temp_dir().join(format!("bhippi-scaffold-{}", EntityId::new()));
    bhippi_engine::scaffold::write_project(&root, "Demo", false).expect("scaffold");

    let index = bhippi_engine::asset::AssetIndex::scan(&root).expect("scan");
    assert!(index.count() > 0, "the scaffold produces indexable assets");

    let report = bhippi_engine::gates::check_assets(&index, &[], true);
    let blockers: Vec<&String> = report
        .blockers()
        .iter()
        .map(|finding| &finding.message)
        .collect();
    assert!(
        report.passes(),
        "a fresh game must not be blocked by its own starter content: {blockers:?}"
    );

    // The starter material and shader are documents the parser accepts, not just files
    // wearing the format marker.
    let material = std::fs::read_to_string(root.join("assets/materials/lit_pbr.mat.json"))
        .expect("material exists");
    bhippi_engine::material::MaterialDocument::parse(&material).expect("material validates");
    let shader = std::fs::read_to_string(root.join("assets/shaders/lit_pbr.shader.json"))
        .expect("shader exists");
    let shader = bhippi_engine::material::ShaderDocument::parse(&shader).expect("shader validates");
    assert!(
        root.join(&shader.source).is_file(),
        "the shader document points at a .wgsl file that exists"
    );

    let _ = std::fs::remove_dir_all(&root);
}

/// And the scaffold's manifest wiring passes the content gates it is checked against.
#[test]
fn a_scaffolded_game_passes_the_content_gates() {
    let root = std::env::temp_dir().join(format!("bhippi-scaffold-{}", EntityId::new()));
    bhippi_engine::scaffold::write_project(&root, "Demo", false).expect("scaffold");
    let manifest = bhippi_engine::manifest::load_manifest(&root)
        .expect("manifest reads")
        .expect("manifest exists");

    let mut scenes = Vec::new();
    // Since ENG-139 the HUD is its own document, so the scaffold writes Main and the level.
    for rel in [
        "assets/scenes/main.bscn.json",
        "assets/scenes/level_01.bscn.json",
    ] {
        let text = std::fs::read_to_string(root.join(rel)).expect("scene exists");
        let doc = bhippi_engine::document::SceneDocument::parse(&text).expect("scene parses");
        scenes.push((rel.to_owned(), doc));
    }

    let report = bhippi_engine::gates::check_project(&root, &manifest, &scenes);
    let blockers: Vec<&String> = report
        .blockers()
        .iter()
        .map(|finding| &finding.message)
        .collect();
    assert!(
        report.passes(),
        "a fresh game must be gate-clean: {blockers:?}"
    );

    // The HUD it ships with is a real, editable document (ENG-139) rather than a scene of
    // entities carrying a magic `UiDocument { layout: "health" }` string.
    let hud = std::fs::read_to_string(root.join("assets/ui/hud_main.hud.json"))
        .expect("the scaffold writes a HUD document");
    let hud = bhippi_engine::hud::HudDocument::parse(&hud).expect("it validates");
    assert!(
        hud.widgets
            .iter()
            .any(|widget| widget.props.contains_key("text")),
        "the starter HUD has text a user can change"
    );

    let _ = std::fs::remove_dir_all(&root);
}

/// ENG-126: the procedural helpers are reachable as verbs, so "scatter forty crates" is one
/// call the engine places, not forty coordinates the model invents.
#[test]
fn scatter_places_a_reproducible_field_of_props_in_one_action() {
    let game_dir = temp_game("scatter");
    let mut sessions = EngineSessions::new();
    let before = sessions.open(&game_dir, LEVEL).expect("open").entity_count;

    let result = run(
        &mut sessions,
        &game_dir,
        "scatter crates across the yard",
        &[json!({
            "kind": "scatter_entities",
            "template": "cube",
            "count": 12,
            "min": [-10.0, 0.5, -10.0],
            "max": [10.0, 0.5, 10.0],
            "min_distance": 2.0,
            "seed": 7,
            "name": "Crate"
        })],
    );
    assert!(result.applied, "{}", result.summary());
    let state = &result.state;
    assert_eq!(state.entity_count, before + 12);

    let doc = sessions.document(&game_dir, LEVEL).expect("open");
    let crates: Vec<_> = doc
        .entities
        .iter()
        .filter(|entity| entity.name.starts_with("Crate "))
        .collect();
    assert_eq!(crates.len(), 12);
    for prop in &crates {
        let pos = &prop.components["Transform"]["pos"];
        for axis in 0..3 {
            let value = pos[axis].as_f64().expect("number");
            assert!(
                (-10.0..=10.0).contains(&value),
                "a scattered prop landed outside the bounds it was given"
            );
        }
    }
    // One undo clears the whole field.
    sessions.undo(&game_dir, LEVEL).expect("undo");
    assert_eq!(
        sessions.open(&game_dir, LEVEL).expect("open").entity_count,
        before
    );

    let _ = std::fs::remove_dir_all(&game_dir);
}

/// A ring of torches should face the fire, and a grid should be countable.
#[test]
fn ring_and_grid_place_what_they_say() {
    let game_dir = temp_game("patterns");
    let mut sessions = EngineSessions::new();
    let before = sessions.open(&game_dir, LEVEL).expect("open").entity_count;

    let result = run(
        &mut sessions,
        &game_dir,
        "torches and pillars",
        &[
            json!({
                "kind": "place_ring",
                "template": "light",
                "center": [0.0, 1.0, 0.0],
                "radius": 6.0,
                "count": 8,
                "face_center": true,
                "name": "Torch"
            }),
            json!({
                "kind": "place_grid",
                "template": "cube",
                "origin": [0.0, 0.0, 0.0],
                "columns": 3,
                "rows": 4,
                "spacing": [2.0, 2.0],
                "name": "Pillar"
            }),
        ],
    );
    assert!(result.applied, "{}", result.summary());
    assert_eq!(result.state.entity_count, before + 8 + 12);

    let doc = sessions.document(&game_dir, LEVEL).expect("open");
    let torch = doc
        .entities
        .iter()
        .find(|entity| entity.name == "Torch 001")
        .expect("torches are numbered from one");
    // Facing the centre means a rotation was computed for it, and it is a unit quaternion.
    let rot = &torch.components["Transform"]["rot"];
    let length: f64 = (0..4)
        .map(|index| {
            let value = rot[index].as_f64().unwrap_or(0.0);
            value * value
        })
        .sum::<f64>()
        .sqrt();
    assert!((length - 1.0).abs() < 1e-4);

    let _ = std::fs::remove_dir_all(&game_dir);
}

/// A pattern nobody can satisfy is refused with a usable hint, rather than half-built.
#[test]
fn an_impossible_placement_is_refused_with_a_hint() {
    let game_dir = temp_game("impossible");
    let mut sessions = EngineSessions::new();
    sessions.open(&game_dir, LEVEL).expect("open");

    let result = run(
        &mut sessions,
        &game_dir,
        "too many",
        &[json!({
            "kind": "scatter_entities",
            "template": "cube",
            "count": 9999,
            "min": [0.0, 0.0, 0.0],
            "max": [1.0, 0.0, 1.0]
        })],
    );
    assert!(!result.applied);
    assert!(result.outcomes[0].hint.is_some());

    let _ = std::fs::remove_dir_all(&game_dir);
}

/// ENG-127: everything the agent creates says so, and names the transaction it came from —
/// which is what makes "find and clean up what the AI added" answerable.
#[test]
fn spawned_entities_record_who_created_them_and_in_which_transaction() {
    let game_dir = temp_game("provenance");
    let mut sessions = EngineSessions::new();
    sessions.open(&game_dir, LEVEL).expect("open");

    let result = run(
        &mut sessions,
        &game_dir,
        "add a crate",
        &[json!({ "kind": "spawn", "template": "cube", "name": "AgentCrate" })],
    );
    assert!(result.applied);

    let doc = sessions.document(&game_dir, LEVEL).expect("open");
    let crate_entity = doc
        .entities
        .iter()
        .find(|entity| entity.name == "AgentCrate")
        .expect("spawned");
    let provenance = &crate_entity.components["Provenance"];
    assert_eq!(provenance["created_by"], "agent");
    assert_eq!(
        provenance["txn"],
        result.edit.as_ref().expect("edit").txn_id
    );
    assert!(
        provenance["at"].as_str().is_some_and(|at| at.contains('T')),
        "an RFC 3339 timestamp"
    );

    // A user edit is recorded as the user's, so the two are distinguishable.
    sessions
        .apply_action(
            &game_dir,
            LEVEL,
            &bhippi_engine::action::EngineAction::Spawn {
                template: "cube".to_owned(),
                at: None,
                parent: None,
                name: Some("UserCrate".to_owned()),
            },
            EngineActor::User,
            "add cube",
            false,
        )
        .expect("user edit");
    let doc = sessions.document(&game_dir, LEVEL).expect("open");
    let user_crate = doc
        .entities
        .iter()
        .find(|entity| entity.name == "UserCrate")
        .expect("spawned");
    assert_eq!(user_crate.components["Provenance"]["created_by"], "user");

    let _ = std::fs::remove_dir_all(&game_dir);
}

/// ENG-192 — two agents, or an agent and the user, cannot silently overwrite each other.
///
/// The property being defended is narrow and important: a batch planned against a scene that
/// has since changed must be **refused**, not applied. Everything else here — the lease, the
/// TTL — exists to make that refusal informative rather than mysterious.
mod multi_agent {
    use bhippi_app::engine::resolve_batch_step as resolve;
    use bhippi_app::engine::session::{BatchRequest, EngineSessions};
    use bhippi_engine::scaffold;
    use bhippi_types::{EngineActor, EntityId};
    use serde_json::json;
    use std::path::{Path, PathBuf};

    const LEVEL: &str = "assets/scenes/level_01.bscn.json";

    fn temp_game(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("bhippi-lock-{label}-{}", EntityId::new()));
        std::fs::create_dir_all(dir.join("assets/scenes")).expect("scene folder");
        std::fs::write(
            dir.join(LEVEL),
            scaffold::starter_scene().dump().expect("dump"),
        )
        .expect("write scene");
        dir
    }

    fn commit(
        sessions: &mut EngineSessions,
        game_dir: &Path,
        owner: Option<&str>,
        base_revision: Option<u32>,
        actor: EngineActor,
    ) -> Result<bhippi_app::engine::session::AppliedBatch, bhippi_app::AppError> {
        sessions.apply_batch(
            BatchRequest {
                game_dir,
                rel_path: LEVEL,
                label: "add a crate",
                actions: &[json!({ "kind": "spawn", "template": "cube" })],
                actor,
                autosave: true,
                owner,
                base_revision,
            },
            resolve,
        )
    }

    #[test]
    fn a_batch_planned_against_a_stale_revision_is_refused_not_applied() {
        let dir = temp_game("stale");
        let mut sessions = EngineSessions::default();
        let opened = sessions.open(&dir, LEVEL).expect("open");
        let planned_at = opened.revision;

        // Someone else edits in between.
        commit(&mut sessions, &dir, None, None, EngineActor::User).expect("the user commits");
        let entities_after_user = sessions
            .document(&dir, LEVEL)
            .expect("document")
            .entity_count();

        let error = commit(
            &mut sessions,
            &dir,
            Some("agent-a"),
            Some(planned_at),
            EngineActor::Agent,
        )
        .expect_err("a stale plan must not be applied");
        assert!(
            error.message.contains("moved since you read it"),
            "{}",
            error.message
        );
        assert!(error
            .hint
            .unwrap_or_default()
            .contains("Nothing was written"));
        assert_eq!(
            sessions
                .document(&dir, LEVEL)
                .expect("document")
                .entity_count(),
            entities_after_user,
            "the refused batch must not have changed the scene"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_second_agent_is_told_who_holds_the_scene_rather_than_racing_it() {
        let dir = temp_game("held");
        let mut sessions = EngineSessions::default();
        sessions.open(&dir, LEVEL).expect("open");

        commit(
            &mut sessions,
            &dir,
            Some("agent-a"),
            None,
            EngineActor::Agent,
        )
        .expect("the first agent takes the scene");
        let error = commit(
            &mut sessions,
            &dir,
            Some("agent-b"),
            None,
            EngineActor::Agent,
        )
        .expect_err("the second must not race it");
        assert!(error.message.contains("agent-a"), "{}", error.message);
        assert!(error.message.contains(LEVEL));

        // The holder itself keeps working.
        commit(
            &mut sessions,
            &dir,
            Some("agent-a"),
            None,
            EngineActor::Agent,
        )
        .expect("the holder may continue");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_user_is_never_locked_out_and_the_agent_is_told_afterwards() {
        let dir = temp_game("user-wins");
        let mut sessions = EngineSessions::default();
        sessions.open(&dir, LEVEL).expect("open");
        commit(
            &mut sessions,
            &dir,
            Some("agent-a"),
            None,
            EngineActor::Agent,
        )
        .expect("agent takes the scene");

        commit(&mut sessions, &dir, None, None, EngineActor::User)
            .expect("a lease must never stop the person using the editor");

        // The agent's next batch was planned before that edit, so it is refused and told to
        // re-read — the whole point of the lease, and the case a lock alone would miss.
        let error = commit(
            &mut sessions,
            &dir,
            Some("agent-a"),
            None,
            EngineActor::Agent,
        )
        .expect_err("a plan built before the user's edit must not be applied");
        assert!(
            error.message.contains("changed under you"),
            "{}",
            error.message
        );

        // Having been told, the retry succeeds: the refusal is a round trip, not a wall.
        commit(
            &mut sessions,
            &dir,
            Some("agent-a"),
            None,
            EngineActor::Agent,
        )
        .expect("the agent may continue once it has re-read the scene");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_agents_own_consecutive_batches_are_not_mistaken_for_interference() {
        let dir = temp_game("consecutive");
        let mut sessions = EngineSessions::default();
        sessions.open(&dir, LEVEL).expect("open");
        for _ in 0..3 {
            commit(
                &mut sessions,
                &dir,
                Some("agent-a"),
                None,
                EngineActor::Agent,
            )
            .expect("an agent may commit repeatedly without rebasing on itself");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_current_revision_commits_normally() {
        let dir = temp_game("fresh");
        let mut sessions = EngineSessions::default();
        let opened = sessions.open(&dir, LEVEL).expect("open");
        let applied = commit(
            &mut sessions,
            &dir,
            Some("agent-a"),
            Some(opened.revision),
            EngineActor::Agent,
        )
        .expect("planning against the current revision is the normal case");
        assert!(applied.result.applied);
        let _ = std::fs::remove_dir_all(&dir);
    }
}

/// ENG-190 — the project's `[agent]` policy is a gate, not a preference.
///
/// The tests go through `apply_batch_in_workspace`, which is the single choke point both
/// agent paths use, so a capability that is denied has to stop a real batch and leave the
/// scene untouched — not merely appear in a settings panel.
mod capabilities {
    use bhippi_engine::capability::{Capability, Decision};
    use bhippi_engine::scaffold;
    use bhippi_types::{EngineActor, EntityId};
    use serde_json::json;
    use std::path::PathBuf;

    fn temp_project(label: &str, tune: impl Fn(&mut bhippi_engine::GameManifest)) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("bhippi-cap-{label}-{}", EntityId::new()));
        for file in scaffold::plan("demo") {
            let path = dir.join(&file.rel_path);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).expect("parent");
            }
            std::fs::write(&path, &file.contents).expect("write");
        }
        let mut manifest = bhippi_engine::manifest::parse_manifest(
            &std::fs::read_to_string(dir.join(bhippi_engine::GAME_MANIFEST_FILE)).expect("read"),
        )
        .expect("the scaffold's manifest parses");
        tune(&mut manifest);
        std::fs::write(
            dir.join(bhippi_engine::GAME_MANIFEST_FILE),
            scaffold::format_manifest(&manifest),
        )
        .expect("write manifest");
        dir
    }

    fn workspace(dir: &std::path::Path) -> String {
        dir.to_string_lossy().into_owned()
    }

    #[test]
    fn a_denied_capability_stops_the_agent_and_writes_nothing() {
        let dir = temp_project("deny", |manifest| {
            manifest
                .agent
                .set(Capability::CreateContent, Decision::Deny);
        });
        let scene = "assets/scenes/level_01.bscn.json";
        let before = std::fs::read_to_string(dir.join(scene)).expect("read scene");

        let error = bhippi_app::engine::apply_batch_in_workspace(
            &workspace(&dir),
            Some(scene),
            "ai:build",
            &[json!({ "kind": "spawn", "template": "cube" })],
            EngineActor::Agent,
            true,
        )
        .expect_err("a denied capability must refuse the batch");

        assert!(
            error.message.contains("create_content"),
            "{}",
            error.message
        );
        assert!(error.message.contains("Bhippi.game.toml"));
        assert_eq!(
            std::fs::read_to_string(dir.join(scene)).expect("read scene"),
            before,
            "a refused batch must leave the file byte-identical"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_gate_does_not_apply_to_the_person_using_the_editor() {
        let dir = temp_project("user", |manifest| {
            manifest
                .agent
                .set(Capability::CreateContent, Decision::Deny);
        });
        let scene = "assets/scenes/level_01.bscn.json";

        let applied = bhippi_app::engine::apply_batch_in_workspace(
            &workspace(&dir),
            Some(scene),
            "add a crate",
            &[json!({ "kind": "spawn", "template": "cube" })],
            EngineActor::User,
            true,
        )
        .expect("a capability switch must never stop the user");
        assert!(applied.result.applied);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_allowed_capability_runs_without_asking() {
        let dir = temp_project("allow", |_| {});
        let scene = "assets/scenes/level_01.bscn.json";

        let applied = bhippi_app::engine::apply_batch_in_workspace(
            &workspace(&dir),
            Some(scene),
            "ai:build",
            &[json!({ "kind": "spawn", "template": "cube" })],
            EngineActor::Agent,
            true,
        )
        .expect("create_content is allowed by default");
        assert!(applied.result.applied);

        // …and the same batch with a delete in it needs approval instead of being refused.
        let verdict = bhippi_app::engine::capability_verdict(
            &dir,
            &[
                json!({ "kind": "spawn", "template": "cube" }),
                json!({ "kind": "delete", "entity": "Crate" }),
            ],
        )
        .expect("verdict");
        assert!(verdict.needs_approval);
        assert!(verdict.denied.is_empty(), "ask is not deny");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_project_with_no_manifest_falls_back_to_the_shipped_defaults() {
        let dir = std::env::temp_dir().join(format!("bhippi-cap-bare-{}", EntityId::new()));
        std::fs::create_dir_all(&dir).expect("dir");
        let verdict = bhippi_app::engine::capability_verdict(
            &dir,
            &[json!({ "kind": "delete", "entity": "Crate" })],
        )
        .expect("a project without a manifest still yields a verdict");
        assert!(verdict.needs_approval);
        let _ = std::fs::remove_dir_all(&dir);
    }
}

/// The model's doctrine and the compiler's host table must not drift (ADR-0030).
///
/// `prompts/chat-engine.md` lists the host functions a script may call. If a function is
/// added in Rust and not there, the model will never use it; if one is removed and the
/// prompt still lists it, the model writes scripts that will not compile. Neither failure
/// shows up anywhere else, so it is asserted here.
mod script_doctrine {
    const ENGINE_PROMPT: &str = include_str!("../../../prompts/chat-engine.md");

    #[test]
    fn the_prompt_lists_every_host_function_the_compiler_accepts() {
        for entry in bhippi_engine::script::HOST_FNS {
            assert!(
                ENGINE_PROMPT.contains(&format!("{}(", entry.name)),
                "prompts/chat-engine.md does not mention the host function `{}`",
                entry.name
            );
        }
    }

    #[test]
    fn the_prompt_names_every_lifecycle_hook() {
        for hook in ["on_start", "on_update", "on_collision", "on_trigger"] {
            assert!(
                ENGINE_PROMPT.contains(hook),
                "prompts/chat-engine.md does not mention `{hook}`"
            );
        }
    }

    #[test]
    fn the_prompt_no_longer_claims_scripts_cannot_run() {
        // This line was true until ENG-176 and is exactly the kind of stale doctrine that
        // makes a model refuse work the engine can now do.
        assert!(
            !ENGINE_PROMPT.contains("nothing executes it"),
            "the prompt still says scripts do not execute"
        );
    }

    #[test]
    fn the_documented_limits_match_the_compiled_ones() {
        assert!(
            ENGINE_PROMPT.contains(&bhippi_engine::script::SCRIPT_STEP_BUDGET.to_string())
                || ENGINE_PROMPT.contains("200 000"),
            "the step budget in the prompt does not match SCRIPT_STEP_BUDGET"
        );
        assert!(
            ENGINE_PROMPT.contains(&bhippi_engine::script::SCRIPT_CALL_DEPTH.to_string()),
            "the call-depth cap in the prompt does not match SCRIPT_CALL_DEPTH"
        );
    }
}

mod game_creation_intent {
    #[test]
    fn scaffolding_succeeds_in_non_empty_workspace_and_enables_engine() {
        let dir = std::env::temp_dir().join(format!(
            "bhippi-game-scaffold-{}",
            bhippi_types::EntityId::new()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create_dir");
        // Place an existing file so directory is not empty (like user's code repo)
        std::fs::write(dir.join("README.md"), "# Existing Project").expect("write");

        let workspace_str = dir.to_string_lossy();
        assert!(bhippi_app::engine::game_dir_of(&workspace_str).is_err());

        // Scaffold with force = true into non-empty directory
        let written = bhippi_engine::scaffold::write_project(&dir, "Test Game", true)
            .expect("scaffold into existing directory succeeds with force=true");
        assert!(written.iter().any(|f| f.contains("Bhippi.game.toml")));

        // Now game_dir_of succeeds
        assert!(bhippi_app::engine::game_dir_of(&workspace_str).is_ok());

        // The default query returns the project's entry scene, which frames the level.
        let query = bhippi_app::engine::query_scene_in_workspace(&workspace_str, None)
            .expect("query starter scene");
        assert_eq!(query.scene_path, "assets/scenes/main.bscn.json");
        assert!(query.entity_count >= 2);

        // The starter level carries the playable content the scaffold promises.
        let level = bhippi_app::engine::query_scene_in_workspace(
            &workspace_str,
            Some("assets/scenes/level_01.bscn.json"),
        )
        .expect("query starter level");
        assert!(level.entity_count >= 4);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
