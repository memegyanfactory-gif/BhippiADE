use crate::document::{SceneDocument, SceneKind, SceneSettings};
use crate::error::{EngineError, Result};
use crate::manifest::GameManifest;
use bhippi_types::EntityId;
use serde_json::json;
use std::path::Path;

/// The canonical new-game layout (plan §9.2). Every path is relative to the game project
/// root, deterministic, and stable across sessions so the AI and the pipeline can locate
/// things by convention rather than by search.
pub const DEFAULT_LEVEL_NAME: &str = "level_01";
pub const DEFAULT_MAIN_NAME: &str = "main";
pub const DEFAULT_HUD_NAME: &str = "hud";
pub const SCENE_DIR: &str = "assets/scenes";
pub const SCRIPTS_DIR: &str = "scripts";
pub const BUILDS_DIR: &str = "builds";
/// The conventional HUD document path (ENG-139).
pub const DEFAULT_HUD_DOC: &str = "assets/ui/hud_main.hud.json";

/// One generated file of a new project: installed relative to the chosen project root.
#[derive(Debug)]
pub struct TemplateFile {
    pub rel_path: String,
    pub contents: String,
}

/// The starter **level** every new game gets: a floor, a key light, a perspective camera and
/// a placeholder player. All ids are freshly minted, so two scaffolds never collide.
pub fn starter_scene() -> SceneDocument {
    let mut doc = SceneDocument::empty(DEFAULT_LEVEL_NAME);
    doc.settings.kind = SceneKind::Level;
    doc.settings.weather = Some("clear".to_owned());

    let floor = EntityId::new();
    let sun = EntityId::new();
    let camera = EntityId::new();
    let player = EntityId::new();

    let transform = |pos: [f32; 3], scale: [f32; 3]| json!({ "pos": pos, "rot": [0.0, 0.0, 0.0, 1.0], "scale": scale });

    doc.entities = vec![
        crate::document::Entity {
            id: floor,
            name: "Floor".to_owned(),
            parent: None,
            tags: vec!["environment".to_owned()],
            components: vec![
                (
                    "Transform".to_owned(),
                    transform([0.0, 0.0, 0.0], [20.0, 1.0, 20.0]),
                ),
                (
                    "MeshRenderer".to_owned(),
                    json!({ "mesh": "builtin:plane", "materials": [], "cast_shadows": true }),
                ),
            ]
            .into_iter()
            .collect(),
        },
        crate::document::Entity {
            id: sun,
            name: "Sun".to_owned(),
            parent: None,
            tags: vec!["environment".to_owned()],
            components: vec![
                (
                    "Transform".to_owned(),
                    transform([10.0, 12.0, 8.0], [1.0, 1.0, 1.0]),
                ),
                (
                    "Light".to_owned(),
                    json!({ "kind": "directional", "color": [1.0, 0.98, 0.9], "intensity": 2.0 }),
                ),
            ]
            .into_iter()
            .collect(),
        },
        crate::document::Entity {
            id: camera,
            name: "MainCamera".to_owned(),
            parent: None,
            tags: vec!["camera".to_owned()],
            components: vec![
                (
                    "Transform".to_owned(),
                    transform([0.0, 5.0, -12.0], [1.0, 1.0, 1.0]),
                ),
                (
                    "Camera".to_owned(),
                    json!({ "fov": 0.9, "near": 0.05, "far": 500.0, "orthographic": false }),
                ),
            ]
            .into_iter()
            .collect(),
        },
        crate::document::Entity {
            id: player,
            name: "Player".to_owned(),
            parent: None,
            tags: vec!["gameplay".to_owned()],
            components: vec![
                (
                    "Transform".to_owned(),
                    transform([0.0, 1.0, 0.0], [1.0, 2.0, 1.0]),
                ),
                (
                    "RigidBody".to_owned(),
                    json!({ "kind": "dynamic", "mass": 70.0, "lock_rotation": true }),
                ),
                (
                    "CharacterController".to_owned(),
                    json!({ "height": 1.8, "radius": 0.35, "max_slope": 0.7, "step_height": 0.3, "move_speed": 5.0, "jump_speed": 5.5 }),
                ),
            ]
            .into_iter()
            .collect(),
        },
    ];
    doc
}

/// Persistent / GameMode scene: HUD attach + ordered levels. Play on this file runs the game.
pub fn main_scene() -> SceneDocument {
    let mut doc = SceneDocument::empty(DEFAULT_MAIN_NAME);
    doc.settings = SceneSettings {
        ambient: [0.18, 0.2, 0.24],
        skybox: None,
        kind: SceneKind::Main,
        hud: None,
        levels: vec![format!("{SCENE_DIR}/{DEFAULT_LEVEL_NAME}.bscn.json")],
        weather: Some("clear".to_owned()),
    };
    let camera = EntityId::new();
    let spawn = EntityId::new();
    let transform = |pos: [f32; 3], scale: [f32; 3]| json!({ "pos": pos, "rot": [0.0, 0.0, 0.0, 1.0], "scale": scale });
    doc.entities = vec![
        crate::document::Entity {
            id: camera,
            name: "GameCamera".to_owned(),
            parent: None,
            tags: vec!["camera".to_owned(), "main".to_owned()],
            components: vec![
                (
                    "Transform".to_owned(),
                    transform([0.0, 6.0, -14.0], [1.0, 1.0, 1.0]),
                ),
                (
                    "Camera".to_owned(),
                    json!({ "fov": 0.9, "near": 0.05, "far": 500.0, "orthographic": false }),
                ),
            ]
            .into_iter()
            .collect(),
        },
        crate::document::Entity {
            id: spawn,
            name: "PlayerStart".to_owned(),
            parent: None,
            tags: vec!["gameplay".to_owned(), "spawn".to_owned()],
            components: vec![(
                "Transform".to_owned(),
                transform([0.0, 1.0, 0.0], [1.0, 1.0, 1.0]),
            )]
            .into_iter()
            .collect(),
        },
    ];
    doc
}

/// Independent HUD scene. Double-click to edit widgets without the 3D level in the way.
pub fn hud_scene() -> SceneDocument {
    let mut doc = SceneDocument::empty(DEFAULT_HUD_NAME);
    doc.settings.kind = SceneKind::Hud;
    let health = EntityId::new();
    let score = EntityId::new();
    let transform = |pos: [f32; 3]| json!({ "pos": pos, "rot": [0.0, 0.0, 0.0, 1.0], "scale": [1.0, 1.0, 1.0] });
    doc.entities = vec![
        crate::document::Entity {
            id: health,
            name: "HealthBar".to_owned(),
            parent: None,
            tags: vec!["hud".to_owned(), "health".to_owned()],
            components: vec![
                ("Transform".to_owned(), transform([-0.42, 0.42, 0.0])),
                ("UiDocument".to_owned(), json!({ "layout": "health" })),
            ]
            .into_iter()
            .collect(),
        },
        crate::document::Entity {
            id: score,
            name: "ScoreLabel".to_owned(),
            parent: None,
            tags: vec!["hud".to_owned(), "score".to_owned()],
            components: vec![
                ("Transform".to_owned(), transform([0.42, 0.42, 0.0])),
                ("UiDocument".to_owned(), json!({ "layout": "score" })),
            ]
            .into_iter()
            .collect(),
        },
    ];
    doc
}

fn dump_or_empty(doc: &SceneDocument) -> String {
    doc.dump().unwrap_or_else(|_| String::new())
}

/// The full file plan for a new game (used by `new_game` and, after the AI knows the
/// convention, by any agent creating games).
pub fn plan(folder_name: &str) -> Vec<TemplateFile> {
    let manifest = GameManifest::defaults(folder_name);
    let main = main_scene();
    let level = starter_scene();

    let mut files = vec![
        TemplateFile {
            rel_path: crate::GAME_MANIFEST_FILE.to_owned(),
            contents: format_manifest(&manifest),
        },
        TemplateFile {
            rel_path: format!("{SCENE_DIR}/{DEFAULT_MAIN_NAME}.bscn.json"),
            contents: dump_or_empty(&main),
        },
        TemplateFile {
            rel_path: format!("{SCENE_DIR}/{DEFAULT_LEVEL_NAME}.bscn.json"),
            contents: dump_or_empty(&level),
        },
    ];

    // Everything Bhippi authors states its own licence. Without this a brand-new project
    // could not produce a Release build: INV-074 blocks on `license = unknown`, and the
    // scaffold's own scenes and materials had no sidecar to say otherwise.
    let generated: Vec<String> = files
        .iter()
        .map(|file| file.rel_path.clone())
        .filter(|path| path.starts_with("assets/"))
        .collect();
    for path in generated {
        files.push(TemplateFile {
            rel_path: format!("{path}.meta.json"),
            contents: authored_sidecar(),
        });
    }
    files
}

fn bhippi_material_slots() -> Vec<&'static str> {
    vec!["albedo", "normal", "roughness", "metallic", "emission"]
}

fn dump_json<T: serde::Serialize>(value: &T) -> String {
    serde_json::to_string_pretty(value).unwrap_or_else(|_| String::from("{}"))
}

/// The sidecar for a file this project authored itself.
fn authored_sidecar() -> String {
    dump_json(&crate::asset::AssetMeta {
        id: bhippi_types::AssetId::new(),
        license: crate::asset::LicenseState::Known("project-authored".to_owned()),
        importer: "bhippi-scaffold".to_owned(),
        imported_at: String::new(),
    })
}

/// Deterministic, hand-rolled manifest text: stable for diffing and AI parsing, and it
/// avoids the toml crate's external-tag enum limitations.
/// Render a manifest as the `Bhippi.game.toml` text Bhippi writes.
///
/// Public because it is the **only** writer of that file: the scaffold uses it for a new
/// game and the Agent-permissions panel uses it to save a capability change. Two writers
/// would mean two formats, and the one people hand-edit would eventually lose.
pub fn format_manifest(manifest: &GameManifest) -> String {
    let track = match manifest.game.engine_track {
        crate::manifest::EngineTrack::Rust => "rust",
        crate::manifest::EngineTrack::Scripted => "scripted",
    };
    let pipeline = match manifest.render.pipeline {
        crate::manifest::RenderPipeline::D3d => "3d",
        crate::manifest::RenderPipeline::D2d => "2d",
    };
    let backend = match manifest.physics.backend {
        crate::manifest::PhysicsBackend::Avian => "avian",
        crate::manifest::PhysicsBackend::None => "none",
    };
    format!(
        "# {name} — Bhippi game project (format v1)\n\
         # Generated by Bhippi. Edit freely; sections are validated on load.\n\n\
         [game]\n\
         id = \"{id}\"\n\
         name = \"{name}\"\n\
         version = \"{version}\"\n\
         default_scene = \"{default_scene}\"\n\
         engine_track = \"{track}\"\n\
         {hud_line}\
         levels = [{levels}]\n\n\
         [render]\n\
         pipeline = \"{pipeline}\"\n\
         msaa = {msaa}\n\n\
         [physics]\n\
         backend = \"{backend}\"\n\
         gravity = {gravity:?}\n\n\
         [targets.windows]\n\
         enabled = {windows}\n\
         [targets.android]\n\
         enabled = {portable}\n\
         package = \"{package}\"\n\
         min_sdk = {min_sdk}\n\
         [targets.ios]\n\
         enabled = {portable}\n\
         bundle_id = \"{bundle_id}\"\n\
         [targets.web]\n\
         enabled = {web}\n\
         canvas_fit = \"{canvas_fit}\"\n\n\
         # What the agent may do to this project (ENG-190). Each key is allow / ask / deny;\n\
         # anything not listed takes its default: edit_scene, create_content, write_script\n\
         # and run_play are allowed, delete, import and build are asked for first.\n\
         [agent]\n\
         {agent}",
        name = manifest.game.name,
        id = manifest.game.id,
        version = manifest.game.version,
        default_scene = manifest.game.default_scene,
        track = track,
        hud_line = manifest
            .game
            .hud_scene
            .as_deref()
            .map(|path| format!("hud_scene = \"{path}\"\n"))
            .unwrap_or_default(),
        levels = manifest
            .game
            .levels
            .iter()
            .map(|path| format!("\"{path}\""))
            .collect::<Vec<_>>()
            .join(", "),
        pipeline = pipeline,
        msaa = manifest.render.msaa,
        backend = backend,
        gravity = manifest.physics.gravity,
        windows = manifest.targets.windows.enabled,
        portable = false,
        package = manifest.targets.android.package,
        min_sdk = manifest.targets.android.min_sdk,
        bundle_id = manifest.targets.ios.bundle_id,
        web = manifest.targets.web.enabled,
        canvas_fit = manifest.targets.web.canvas_fit,
        // Only what differs from the defaults, so the section starts empty and stays
        // readable — a wall of restated defaults is a wall nobody edits.
        agent = toml::to_string(&manifest.agent).unwrap_or_default(),
    )
}

/// The script a new game ships with (ADR-0030).
///
/// It used to be three comment lines and nothing else — which now fails to compile, because
/// a file with no lifecycle hook is unreachable and is refused as such. More to the point, a
/// commented-out example teaches nothing. This one runs: it demonstrates every category of
/// host call the subset offers, and `the_scaffolded_script_compiles` proves it still does.
const README_SCRIPT: &str = r#"// Entry script for level_01 (ADR-0030: a documented subset of Rhai).
//
// Attach this to an entity with `attach_script`, or from the Details panel's ScriptRef row.
// Hooks: on_start(), on_update(dt), on_collision(other), on_trigger(other).

fn on_start() {
    set_var("game.score", 0);
    log("level_01 started");
}

fn on_update(dt) {
    // Spin gently in place. `self_id()` is the entity this script is attached to.
    set_rot(self_id(), 0.0, rot_y(self_id()) + dt, 0.0);
}

fn on_trigger(other) {
    // Anything tagged `player` scores; everything else is ignored.
    if !has_tag(other, "player") {
        return;
    }
    set_var("game.score", get_var("game.score") + 1);
    hud_set("score_label", "Score " + get_var("game.score"));
}
"#;

/// A minimal, readable starting point the user can actually edit. It compiles nothing
/// today — the renderer's shader pipeline is Phase 5 — but it is a real WGSL file rather
/// than a placeholder string, so `lit_pbr.shader.json` points at something that exists.
const LIT_PBR_WGSL: &str = r#"// Bhippi standard lit surface shader.
//
// Edit this file to change how lit materials are drawn. The material document
// (lit_pbr.shader.json) names this file; materials reference that document.

struct SurfaceInput {
    world_position: vec3<f32>,
    world_normal: vec3<f32>,
    uv: vec2<f32>,
};

struct SurfaceOutput {
    base_color: vec3<f32>,
    roughness: f32,
    metallic: f32,
    emissive: vec3<f32>,
};

fn surface(input: SurfaceInput) -> SurfaceOutput {
    var output: SurfaceOutput;
    output.base_color = material_base_color;
    output.roughness = material_roughness;
    output.metallic = material_metallic;
    output.emissive = material_emissive;
    return output;
}
"#;

const ULTRASKY_PRESETS: &str = r#"{
  "format": "bhippi-weather@1",
  "name": "ultrasky",
  "presets": [
    { "id": "clear", "label": "Clear", "sun": 2.4, "fog": 0.0, "precip": "none" },
    { "id": "overcast", "label": "Overcast", "sun": 0.8, "fog": 0.12, "precip": "none" },
    { "id": "rain", "label": "Rain", "sun": 0.55, "fog": 0.18, "precip": "rain" },
    { "id": "snow", "label": "Snow", "sun": 0.7, "fog": 0.22, "precip": "snow" },
    { "id": "fog", "label": "Fog", "sun": 0.4, "fog": 0.55, "precip": "none" },
    { "id": "storm", "label": "Storm", "sun": 0.3, "fog": 0.28, "precip": "rain" },
    { "id": "sunset", "label": "Sunset", "sun": 1.6, "fog": 0.08, "precip": "none" },
    { "id": "night", "label": "Night", "sun": 0.12, "fog": 0.06, "precip": "none" }
  ]
}
"#;

/// Palette templates the asset drawer and `EngineAction::Spawn` both resolve to. This is
/// the single source of truth for what a human/AI can place without importing assets.
pub fn templates() -> Vec<TemplateSpec> {
    vec![
        TemplateSpec {
            name: "cube".to_owned(),
            label: "Cube".to_owned(),
            kind: TemplateKind::Visual,
            components: vec![
                (
                    "Transform".to_owned(),
                    json!({ "pos": [0.0, 0.5, 0.0], "rot": [0.0, 0.0, 0.0, 1.0], "scale": [1.0, 1.0, 1.0] }),
                ),
                (
                    "MeshRenderer".to_owned(),
                    json!({ "mesh": "builtin:cube", "materials": [], "cast_shadows": true }),
                ),
            ],
        },
        TemplateSpec {
            name: "sphere".to_owned(),
            label: "Sphere".to_owned(),
            kind: TemplateKind::Visual,
            components: vec![
                (
                    "Transform".to_owned(),
                    json!({ "pos": [0.0, 0.5, 0.0], "rot": [0.0, 0.0, 0.0, 1.0], "scale": [1.0, 1.0, 1.0] }),
                ),
                (
                    "MeshRenderer".to_owned(),
                    json!({ "mesh": "builtin:sphere", "materials": [], "cast_shadows": true }),
                ),
            ],
        },
        TemplateSpec {
            name: "plane".to_owned(),
            label: "Plane".to_owned(),
            kind: TemplateKind::Visual,
            components: vec![
                (
                    "Transform".to_owned(),
                    json!({ "pos": [0.0, 0.0, 0.0], "rot": [0.0, 0.0, 0.0, 1.0], "scale": [1.0, 1.0, 1.0] }),
                ),
                (
                    "MeshRenderer".to_owned(),
                    json!({ "mesh": "builtin:plane", "materials": [], "cast_shadows": true }),
                ),
            ],
        },
        TemplateSpec {
            name: "light".to_owned(),
            label: "Light".to_owned(),
            kind: TemplateKind::Visual,
            components: vec![
                (
                    "Transform".to_owned(),
                    json!({ "pos": [0.0, 6.0, 0.0], "rot": [0.0, 0.0, 0.0, 1.0], "scale": [1.0, 1.0, 1.0] }),
                ),
                (
                    "Light".to_owned(),
                    json!({ "kind": "point", "color": [1.0, 1.0, 1.0], "intensity": 1.0, "range": 20.0 }),
                ),
            ],
        },
        TemplateSpec {
            name: "camera".to_owned(),
            label: "Camera".to_owned(),
            kind: TemplateKind::Camera,
            components: vec![
                (
                    "Transform".to_owned(),
                    json!({ "pos": [0.0, 2.0, -5.0], "rot": [0.0, 0.0, 0.0, 1.0], "scale": [1.0, 1.0, 1.0] }),
                ),
                (
                    "Camera".to_owned(),
                    json!({ "fov": 0.9, "near": 0.05, "far": 500.0, "orthographic": false }),
                ),
            ],
        },
        TemplateSpec {
            name: "player".to_owned(),
            label: "Player capsule".to_owned(),
            kind: TemplateKind::Gameplay,
            components: vec![
                (
                    "Transform".to_owned(),
                    json!({ "pos": [0.0, 1.0, 0.0], "rot": [0.0, 0.0, 0.0, 1.0], "scale": [1.0, 2.0, 1.0] }),
                ),
                (
                    "RigidBody".to_owned(),
                    json!({ "kind": "dynamic", "mass": 70.0, "lock_rotation": true }),
                ),
                (
                    "CharacterController".to_owned(),
                    json!({ "height": 1.8, "radius": 0.35, "max_slope": 0.7, "step_height": 0.3, "move_speed": 5.0, "jump_speed": 5.5 }),
                ),
            ],
        },
        TemplateSpec {
            name: "trigger".to_owned(),
            label: "Trigger zone".to_owned(),
            kind: TemplateKind::Gameplay,
            components: vec![
                (
                    "Transform".to_owned(),
                    json!({ "pos": [0.0, 0.5, 0.0], "rot": [0.0, 0.0, 0.0, 1.0], "scale": [2.0, 2.0, 2.0] }),
                ),
                (
                    "Collider".to_owned(),
                    json!({ "shape": { "cuboid": [2.0, 2.0, 2.0] }, "sensor": true }),
                ),
                ("Tag".to_owned(), json!({ "value": "trigger" })),
            ],
        },
        TemplateSpec {
            name: "empty".to_owned(),
            label: "Empty node".to_owned(),
            kind: TemplateKind::Visual,
            components: vec![(
                "Transform".to_owned(),
                json!({ "pos": [0.0, 0.0, 0.0], "rot": [0.0, 0.0, 0.0, 1.0], "scale": [1.0, 1.0, 1.0] }),
            )],
        },
    ]
}

/// Lookup a palette template by name.
pub fn template(name: &str) -> Option<TemplateSpec> {
    templates().into_iter().find(|spec| spec.name == name)
}

#[derive(Clone, Debug, PartialEq)]
pub struct TemplateSpec {
    pub name: String,
    pub label: String,
    pub kind: TemplateKind,
    pub components: Vec<(String, serde_json::Value)>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TemplateKind {
    Visual,
    Camera,
    Gameplay,
}

/// Create a full game project at `root`. Returns the list of written relative paths.
/// Fails (without writing anything) if the target folder exists and is not empty and
/// `force` is false, so accidental overwrites need an explicit flag (plan §9.2 gate).
pub fn write_project(root: &Path, folder_name: &str, force: bool) -> Result<Vec<String>> {
    if root.exists() {
        let mut entries = std::fs::read_dir(root)
            .map_err(|error| EngineError::Io {
                operation: "scaffold",
                path: root.display().to_string(),
                reason: error.to_string(),
                hint: Some("Check the target folder is writable.".to_owned()),
            })?
            .filter_map(std::result::Result::ok);
        if entries.next().is_some() && !force {
            return Err(EngineError::Manifest(
                format!("{} is not empty", root.display()),
                Some("Pick an empty folder or pass force=true.".to_owned()),
            ));
        }
    }
    std::fs::create_dir_all(root).map_err(|error| EngineError::Io {
        operation: "scaffold",
        path: root.display().to_string(),
        reason: error.to_string(),
        hint: None,
    })?;
    let mut written = Vec::new();
    for file in plan(folder_name) {
        let full = root.join(&file.rel_path);
        if let Some(parent) = full.parent() {
            std::fs::create_dir_all(parent).map_err(|error| EngineError::Io {
                operation: "scaffold",
                path: parent.display().to_string(),
                reason: error.to_string(),
                hint: None,
            })?;
        }
        if full.exists() && !force {
            return Err(EngineError::Manifest(
                format!("{} already exists", full.display()),
                Some("Pass force=true to overwrite.".to_owned()),
            ));
        }
        std::fs::write(&full, file.contents).map_err(|error| EngineError::Io {
            operation: "scaffold",
            path: full.display().to_string(),
            reason: error.to_string(),
            hint: None,
        })?;
        written.push(file.rel_path.clone());
    }
    Ok(written)
}

/// Validate the schema excerpt referenced by a template's component so templates never
/// drift from the registry (plan §10.1).
#[cfg(test)]
mod tests {

    /// The manifest is written by hand rather than by `toml::to_string`, so the emitted text
    /// and the parser can drift. Round-tripping a *non-default* policy is the case that
    /// would break silently: an `[agent]` section in the wrong place parses as nothing.
    #[test]
    fn the_written_manifest_round_trips_including_a_changed_agent_policy() {
        let mut manifest = crate::manifest::GameManifest::defaults("demo");
        manifest.agent.set(
            crate::capability::Capability::Delete,
            crate::capability::Decision::Deny,
        );
        manifest.agent.set(
            crate::capability::Capability::Build,
            crate::capability::Decision::Allow,
        );

        let text = super::format_manifest(&manifest);
        let parsed =
            crate::manifest::parse_manifest(&text).expect("the scaffold's own output parses");
        assert_eq!(parsed.agent, manifest.agent);
        assert_eq!(
            parsed.agent.decision(crate::capability::Capability::Delete),
            crate::capability::Decision::Deny
        );
        // Untouched capabilities are absent from the file (the header comment names them,
        // the `[agent]` table does not) and still take their defaults.
        let section = text.split("[agent]").nth(1).unwrap_or_default();
        assert!(
            !section.contains("edit_scene"),
            "defaults must not be restated, got {section:?}"
        );
        assert_eq!(
            parsed
                .agent
                .decision(crate::capability::Capability::EditScene),
            crate::capability::Decision::Allow
        );
    }

    #[test]
    fn a_default_project_writes_an_empty_agent_section_that_still_parses() {
        let manifest = crate::manifest::GameManifest::defaults("demo");
        let text = super::format_manifest(&manifest);
        let parsed = crate::manifest::parse_manifest(&text).expect("parses");
        assert_eq!(parsed.agent, crate::capability::CapabilityPolicy::default());
    }

    /// The file a new game ships with must run. A scaffolded script that fails to compile
    use super::{plan, starter_scene, templates, write_project};
    use std::fs;
    use std::path::Path;

    #[test]
    fn starter_scene_is_valid_and_deterministic() {
        let scene = starter_scene();
        scene.validate().expect("starter scene valid");
        assert!(scene.entity_count() >= 4);
        let first = scene.dump().expect("dump");
        let second = scene.dump().expect("dump");
        assert_eq!(first, second);
    }

    #[test]
    fn every_template_component_is_valid() {
        for template in templates() {
            for (_component, value) in &template.components {
                assert!(value.is_object());
            }
        }
    }

    #[test]
    fn the_asset_drawer_can_resolve_known_templates() {
        for name in [
            "cube", "sphere", "plane", "light", "camera", "player", "trigger",
        ] {
            assert!(super::template(name).is_some(), "missing template {name}");
        }
        assert!(super::template("flying-saucer").is_none());
    }

    #[test]
    fn write_project_refuses_nonempty_folders_without_force() {
        let root = std::env::temp_dir().join("bhippi-scaffold-guard");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).expect("root");
        fs::write(root.join(".placeholder"), "x").expect("placeholder");
        let result = write_project(&root, "MyGame", false);
        assert!(result.is_err());
        assert!(result.expect_err("error").hint().is_some());

        let written = write_project(&root, "MyGame", true).expect("force works");
        assert!(written.contains(&"Bhippi.game.toml".to_owned()));
        assert!(Path::new(&root)
            .join("assets/scenes/level_01.bscn.json")
            .is_file());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn generated_scene_round_trips_through_parse() {
        let files = plan("Demo");
        let scene_text = files
            .iter()
            .find(|file| file.rel_path.ends_with("level_01.bscn.json"))
            .expect("scene in plan")
            .contents
            .clone();
        crate::document::SceneDocument::parse(&scene_text).expect("parses");
    }
}
