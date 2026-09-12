//! A test states its preconditions with `unwrap`/`expect`: a panic here is a failing
//! test, not a crashed app. The workspace `deny` stands everywhere else.
#![allow(clippy::expect_used, clippy::unwrap_used)]

//! What the agent could not build, and now can.
//!
//! The owner, several times: *use a joystick in the game* — and it never happened. The reason
//! was not the model. Three things were missing from the typed vocabulary, and each one on its
//! own was enough to stop it:
//!
//! 1. **No sub-resource could be created.** Every `CollisionShape`'s `shape`, every
//!    `MeshInstance3D`'s `mesh`, every `Panel`'s `StyleBoxFlat` is a `SubResource(…)`, and
//!    nothing in the vocabulary could mint one. The agent could add a collider and could not
//!    give it a shape.
//! 2. **Input actions were keyboard-only.** `add_input_action` took `keycodes: Vec<u32>` and
//!    wrote `InputEventKey` literals. An analogue stick is an `InputEventJoypadMotion`, so a
//!    gamepad joystick was literally unsayable.
//! 3. **Nothing in the catalogue answered "joystick".** A hundred and three presets across
//!    seventeen categories, and not one for input or on-screen controls.
//!
//! These tests build the things that used to be impossible, against a real project on disk,
//! and read the result back through the parser.

use bhippi_engine::godot::action::{
    apply_changeset, lower, GodotAction, GodotActionBatch, InputEventSpec,
};
use bhippi_engine::godot::project::GodotProjectFile;
use bhippi_engine::godot::scaffold::{write_project, ProjectTemplate};
use bhippi_engine::godot::scene::GodotScene;
use bhippi_engine::godot::tscn::TscnValue;
use std::path::{Path, PathBuf};

fn project(name: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("bhippi-controls-{name}-{}", ulid::Ulid::new()));
    let _ = std::fs::remove_dir_all(&root);
    write_project(&root, "Controls", ProjectTemplate::TopDown2D, true).expect("scaffold");
    root
}

/// Lower and apply, failing the test with the engine's own message.
fn run(root: &Path, label: &str, actions: Vec<GodotAction>) {
    let batch = GodotActionBatch::new(label, actions);
    let changeset = lower(root, &batch).unwrap_or_else(|error| panic!("{label}: {error}"));
    apply_changeset(root, &changeset).expect("apply");
}

fn scene_at(root: &Path, rel: &str) -> GodotScene {
    let text = std::fs::read_to_string(root.join(rel)).expect("scene readable");
    GodotScene::parse(&text.replace("\r\n", "\n")).expect("scene parses")
}

// ── the thing that was asked for ─────────────────────────────────────────────────

/// The joystick, built the way a model would build it now: a `CanvasLayer`, a `Control` that
/// draws itself, and a script. No imported art, because the agent still cannot import art —
/// the stick is drawn, which is what makes it expressible at all.
#[test]
fn the_agent_can_build_a_touch_joystick() {
    let root = project("joystick");

    run(
        &root,
        "add a touch joystick",
        vec![
            GodotAction::WriteScript {
                path: "scripts/touch_joystick.gd".to_owned(),
                source: TOUCH_JOYSTICK.to_owned(),
            },
            GodotAction::AddNode {
                scene: "scenes/main.tscn".to_owned(),
                parent: ".".to_owned(),
                name: "TouchControls".to_owned(),
                type_: "CanvasLayer".to_owned(),
                properties: Vec::new(),
                groups: Vec::new(),
            },
            GodotAction::AddNode {
                scene: "scenes/main.tscn".to_owned(),
                parent: "TouchControls".to_owned(),
                name: "Joystick".to_owned(),
                type_: "Control".to_owned(),
                properties: vec![
                    ("offset_left".to_owned(), TscnValue::Float(80.0)),
                    ("offset_top".to_owned(), TscnValue::Float(-260.0)),
                    ("radius".to_owned(), TscnValue::Float(110.0)),
                    ("knob_radius".to_owned(), TscnValue::Float(44.0)),
                    ("dead_zone".to_owned(), TscnValue::Float(0.2)),
                ],
                groups: vec!["touch_controls".to_owned()],
            },
            GodotAction::AttachScript {
                scene: "scenes/main.tscn".to_owned(),
                path: "TouchControls/Joystick".to_owned(),
                script_res_path: "res://scripts/touch_joystick.gd".to_owned(),
            },
        ],
    );

    let scene = scene_at(&root, "scenes/main.tscn");
    let joystick = scene
        .node("TouchControls/Joystick")
        .expect("the joystick is in the scene");
    assert_eq!(joystick.type_.as_deref(), Some("Control"));
    assert_eq!(
        joystick.script.as_deref(),
        Some("res://scripts/touch_joystick.gd"),
        "the script is attached and its ext_resource resolved"
    );
    assert!(joystick
        .groups
        .iter()
        .any(|group| group == "touch_controls"));
    assert!(root.join("scripts/touch_joystick.gd").exists());

    std::fs::remove_dir_all(&root).ok();
}

/// The other half of "use a joystick": a real controller. This is the shape that could not be
/// expressed at all before — an action bound to a stick *axis* rather than to a key.
#[test]
fn the_agent_can_bind_an_analogue_stick_to_an_input_action() {
    let root = project("gamepad");

    run(
        &root,
        "bind the left stick",
        vec![
            GodotAction::AddInputAction {
                name: "move_right".to_owned(),
                // Keyboard and gamepad on one action, which is how a real game binds it.
                keycodes: vec![68],
                events: vec![InputEventSpec::JoypadMotion {
                    axis: 0,
                    axis_value: 1.0,
                    device: None,
                }],
                deadzone: Some(0.2),
            },
            GodotAction::AddInputAction {
                name: "jump".to_owned(),
                keycodes: vec![32],
                events: vec![InputEventSpec::JoypadButton {
                    button: 0,
                    device: None,
                }],
                deadzone: None,
            },
        ],
    );

    let text = std::fs::read_to_string(root.join("project.godot")).expect("project readable");
    assert!(
        text.contains("InputEventJoypadMotion"),
        "the stick axis is written as a real joypad motion event"
    );
    assert!(text.contains("\"axis\":0"));
    assert!(text.contains("InputEventJoypadButton"));
    assert!(
        text.contains("InputEventKey"),
        "the keyboard binding survives beside the gamepad one"
    );

    // And Godot's own reader would find both actions: the file still parses as a project.
    let parsed = GodotProjectFile::parse(&text.replace("\r\n", "\n")).expect("project parses");
    let actions = parsed.input_actions();
    assert!(actions.iter().any(|name| name == "move_right"));
    assert!(actions.iter().any(|name| name == "jump"));

    std::fs::remove_dir_all(&root).ok();
}

// ── the gap underneath it ────────────────────────────────────────────────────────

/// The one that blocked far more than joysticks: without it the agent could add a collider
/// and never give it a shape, add a mesh node and never give it a mesh.
#[test]
fn the_agent_can_give_a_collider_a_shape_and_a_mesh_a_mesh() {
    let root = project("subresources");

    run(
        &root,
        "add a solid crate",
        vec![
            GodotAction::AddSubResource {
                scene: "scenes/main.tscn".to_owned(),
                id: "RectangleShape2D_crate".to_owned(),
                type_: "RectangleShape2D".to_owned(),
                properties: vec![("size".to_owned(), TscnValue::Vector2(64.0, 64.0))],
            },
            GodotAction::AddNode {
                scene: "scenes/main.tscn".to_owned(),
                parent: ".".to_owned(),
                name: "Crate".to_owned(),
                type_: "StaticBody2D".to_owned(),
                properties: Vec::new(),
                groups: Vec::new(),
            },
            GodotAction::AddNode {
                scene: "scenes/main.tscn".to_owned(),
                parent: "Crate".to_owned(),
                name: "Shape".to_owned(),
                type_: "CollisionShape2D".to_owned(),
                // The reference that used to be impossible to satisfy.
                properties: vec![(
                    "shape".to_owned(),
                    TscnValue::SubResource("RectangleShape2D_crate".to_owned()),
                )],
                groups: Vec::new(),
            },
        ],
    );

    let text = std::fs::read_to_string(root.join("scenes/main.tscn")).expect("scene readable");
    assert!(text.contains("[sub_resource type=\"RectangleShape2D\" id=\"RectangleShape2D_crate\"]"));
    assert!(text.contains("shape = SubResource(\"RectangleShape2D_crate\")"));

    let scene = scene_at(&root, "scenes/main.tscn");
    let shape = scene.node("Crate/Shape").expect("the collider is there");
    assert!(
        shape.properties.iter().any(|(name, _)| name == "shape"),
        "the collider carries its shape — the exact thing BHP-INS-802 reports when it does not"
    );

    std::fs::remove_dir_all(&root).ok();
}

/// A reference to a sub-resource that does not exist is refused at the batch, rather than
/// written and found later by a scan.
#[test]
fn a_dangling_sub_resource_reference_is_refused_with_the_verb_that_fixes_it() {
    let root = project("dangling");

    let batch = GodotActionBatch::new(
        "a collider pointing at nothing",
        vec![GodotAction::AddNode {
            scene: "scenes/main.tscn".to_owned(),
            parent: ".".to_owned(),
            name: "Ghost".to_owned(),
            type_: "CollisionShape2D".to_owned(),
            properties: vec![(
                "shape".to_owned(),
                TscnValue::SubResource("NothingLikeThis".to_owned()),
            )],
            groups: Vec::new(),
        }],
    );
    let error = lower(&root, &batch).expect_err("a dangling reference is refused");
    let message = error.to_string();
    assert!(
        message.contains("NothingLikeThis"),
        "the refusal names the id: {message}"
    );
    assert!(
        error
            .error
            .hint()
            .is_some_and(|hint| hint.contains("add_sub_resource")),
        "and names the verb that would have created it"
    );

    // Nothing was written.
    let scene = scene_at(&root, "scenes/main.tscn");
    assert!(scene.node("Ghost").is_none());

    std::fs::remove_dir_all(&root).ok();
}

/// The catalogue can answer the question that used to have no answer.
#[test]
fn asking_the_catalogue_for_a_joystick_now_finds_one() {
    let joystick = bhippi_engine::intent::catalog::preset("preset.control.touch_joystick")
        .expect("the touch joystick is in the catalogue");
    assert_eq!(joystick.title, "Touch joystick");
    assert!(
        joystick
            .properties
            .iter()
            .any(|spec| spec.name == "dead_zone"),
        "and it is tunable"
    );

    let controls: Vec<&str> = bhippi_engine::intent::catalog::presets()
        .iter()
        .filter(|card| card.id.starts_with("preset.control."))
        .map(|card| card.id)
        .collect();
    assert_eq!(
        controls,
        vec![
            "preset.control.touch_joystick",
            "preset.control.touch_buttons",
        ],
        "the control category exists and is the one the model will find"
    );

    // A physical gamepad is deliberately *not* a preset: it places no nodes, and every
    // preset in this catalogue builds something. It is an `add_input_action` with
    // `joypad_motion` events, and §5 of `prompts/chat-engine.md` is where the model is told.
    assert!(
        bhippi_engine::intent::catalog::preset("preset.control.gamepad").is_none(),
        "binding a controller is a verb call, not a node tree"
    );
}

/// A drawn stick: no texture, no imported art, and therefore expressible by an agent that
/// cannot import assets. This is the source the test above writes; it is here so the shape
/// the preset promises is a real, compiling script rather than a description of one.
const TOUCH_JOYSTICK: &str = r#"extends Control

## An on-screen analogue stick. Drawn rather than textured, so it needs no imported art.
## Read `value` from a player script, or let it drive input actions directly.

@export var radius: float = 110.0
@export var knob_radius: float = 44.0
@export var dead_zone: float = 0.2

## -1..1 on both axes, dead zone already applied.
var value: Vector2 = Vector2.ZERO

var _touch_index: int = -1
var _knob: Vector2 = Vector2.ZERO


func _ready() -> void:
	custom_minimum_size = Vector2(radius * 2.0, radius * 2.0)


func _draw() -> void:
	var centre := Vector2(radius, radius)
	draw_circle(centre, radius, Color(1, 1, 1, 0.10))
	draw_arc(centre, radius, 0.0, TAU, 48, Color(1, 1, 1, 0.35), 2.0)
	draw_circle(centre + _knob, knob_radius, Color(1, 1, 1, 0.55))


func _gui_input(event: InputEvent) -> void:
	if event is InputEventScreenTouch:
		if event.pressed:
			_touch_index = event.index
			_move_to(event.position)
		elif event.index == _touch_index:
			_release()
	elif event is InputEventScreenDrag and event.index == _touch_index:
		_move_to(event.position)
	elif event is InputEventMouseButton:
		if event.pressed:
			_touch_index = 0
			_move_to(event.position)
		else:
			_release()
	elif event is InputEventMouseMotion and _touch_index == 0:
		_move_to(event.position)


func _move_to(local: Vector2) -> void:
	var centre := Vector2(radius, radius)
	var offset := local - centre
	if offset.length() > radius:
		offset = offset.normalized() * radius
	_knob = offset
	var raw := offset / radius
	value = Vector2.ZERO if raw.length() < dead_zone else raw
	queue_redraw()


func _release() -> void:
	_touch_index = -1
	_knob = Vector2.ZERO
	value = Vector2.ZERO
	queue_redraw()
"#;
