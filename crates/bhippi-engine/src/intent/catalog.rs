//! The vocabulary the intent compiler is allowed to name.
//!
//! Two static tables live here and nowhere else:
//!
//! * [`GODOT_CLASSES`] — the Godot 4 node classes an archetype or a fast-path edit may
//!   address. Nothing outside this list is a legal class id.
//! * [`presets`] — the Bhippi preset cards (`preset.<domain>.<name>`), each naming the
//!   Godot nodes it builds and the properties it exposes.
//!
//! Every id an archetype pack names is checked against these tables by
//! [`crate::intent::archetype::Archetype::validate`], so a typo in a pack is a test
//! failure rather than a build that silently drops a system. A later ticket binds these
//! cards to the capability registry; the registry is deliberately not consulted here so
//! the intent pass stays free of registry state and runs at zero token cost.

/// Separator-checked prefix every Bhippi preset id carries.
pub const PRESET_PREFIX: &str = "preset.";

/// Godot 4 node classes the compiler may name. Anything absent is refused rather than
/// passed through: an unknown class reaches the scene writer as an unbuildable node.
pub const GODOT_CLASSES: &[&str] = &[
    "AnimationPlayer",
    "Area2D",
    "Area3D",
    "AudioStreamPlayer",
    "AudioStreamPlayer2D",
    "AudioStreamPlayer3D",
    "Button",
    "CSGBox3D",
    "CSGCylinder3D",
    "CSGSphere3D",
    "Camera2D",
    "Camera3D",
    "CanvasLayer",
    "CharacterBody2D",
    "CharacterBody3D",
    "CollisionShape2D",
    "CollisionShape3D",
    "Control",
    "DirectionalLight3D",
    "GPUParticles2D",
    "GPUParticles3D",
    "Label",
    "Marker3D",
    "MeshInstance3D",
    "NavigationAgent3D",
    "NavigationRegion3D",
    "Node2D",
    "Node3D",
    "OmniLight3D",
    "Path3D",
    "PathFollow3D",
    "ProgressBar",
    "RayCast3D",
    "RigidBody2D",
    "RigidBody3D",
    "Sprite2D",
    "StaticBody2D",
    "StaticBody3D",
    "TextureRect",
    "TileMapLayer",
    "Timer",
    "VehicleBody3D",
    "WorldEnvironment",
];

/// How a preset property is typed. `Choice` carries its closed option set so the fast
/// path can refuse a value the preset would reject.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PropertyKind {
    Number,
    Bool,
    Text,
    Color,
    Choice(&'static [&'static str]),
}

/// One tunable knob on a preset. `default` is the authored text form so the table stays a
/// `const` — the fast path parses it only when it needs a starting value.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PropertySpec {
    pub name: &'static str,
    pub kind: PropertyKind,
    pub default: &'static str,
    pub unit: Option<&'static str>,
    pub min: Option<&'static str>,
    pub max: Option<&'static str>,
}

/// A reviewable unit of game construction: one preset builds one coherent set of Godot
/// nodes and exposes the handful of properties a follow-up prompt is likely to touch.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PresetCard {
    pub id: &'static str,
    pub title: &'static str,
    pub purpose: &'static str,
    pub godot_nodes: &'static [&'static str],
    pub properties: &'static [PropertySpec],
}

const fn number(name: &'static str, default: &'static str) -> PropertySpec {
    PropertySpec {
        name,
        kind: PropertyKind::Number,
        default,
        unit: None,
        min: None,
        max: None,
    }
}

const fn measured(
    name: &'static str,
    default: &'static str,
    unit: &'static str,
    min: &'static str,
    max: &'static str,
) -> PropertySpec {
    PropertySpec {
        name,
        kind: PropertyKind::Number,
        default,
        unit: Some(unit),
        min: Some(min),
        max: Some(max),
    }
}

const fn boolean(name: &'static str, default: &'static str) -> PropertySpec {
    PropertySpec {
        name,
        kind: PropertyKind::Bool,
        default,
        unit: None,
        min: None,
        max: None,
    }
}

const fn choice(
    name: &'static str,
    options: &'static [&'static str],
    default: &'static str,
) -> PropertySpec {
    PropertySpec {
        name,
        kind: PropertyKind::Choice(options),
        default,
        unit: None,
        min: None,
        max: None,
    }
}

const fn colour(name: &'static str, default: &'static str) -> PropertySpec {
    PropertySpec {
        name,
        kind: PropertyKind::Color,
        default,
        unit: None,
        min: None,
        max: None,
    }
}

const fn text(name: &'static str, default: &'static str) -> PropertySpec {
    PropertySpec {
        name,
        kind: PropertyKind::Text,
        default,
        unit: None,
        min: None,
        max: None,
    }
}

const DIFFICULTY: &[&str] = &["easy", "normal", "hard"];

const PRESETS: &[PresetCard] = &[
    // ---------------------------------------------------------------- player
    PresetCard {
        id: "preset.player.third_person_3d",
        title: "Third-person character",
        purpose: "Walk, run and jump behind a follow camera in a 3D world.",
        godot_nodes: &["CharacterBody3D", "CollisionShape3D", "MeshInstance3D"],
        properties: &[
            measured("speed", "6.0", "m/s", "1.0", "30.0"),
            measured("jump_velocity", "5.5", "m/s", "1.0", "20.0"),
            measured("gravity", "16.0", "m/s2", "1.0", "60.0"),
            measured("acceleration", "10.0", "m/s2", "1.0", "80.0"),
        ],
    },
    PresetCard {
        id: "preset.player.platformer_2d",
        title: "2D platformer character",
        purpose: "Side-on run and jump with coyote time and variable jump height.",
        godot_nodes: &["CharacterBody2D", "CollisionShape2D", "Sprite2D"],
        properties: &[
            measured("speed", "220.0", "px/s", "40.0", "900.0"),
            measured("jump_velocity", "420.0", "px/s", "80.0", "1200.0"),
            measured("gravity", "1200.0", "px/s2", "100.0", "4000.0"),
            measured("coyote_time", "0.12", "s", "0.0", "0.5"),
        ],
    },
    PresetCard {
        id: "preset.player.fps",
        title: "First-person shooter body",
        purpose: "Mouse-look movement with sprint, crouch and a hitscan weapon mount.",
        godot_nodes: &[
            "CharacterBody3D",
            "Camera3D",
            "RayCast3D",
            "CollisionShape3D",
        ],
        properties: &[
            measured("speed", "7.0", "m/s", "1.0", "30.0"),
            measured("look_sensitivity", "0.25", "deg/px", "0.02", "2.0"),
            measured("jump_velocity", "4.5", "m/s", "1.0", "20.0"),
            measured("max_health", "100.0", "hp", "1.0", "1000.0"),
        ],
    },
    PresetCard {
        id: "preset.player.kart",
        title: "Arcade kart",
        purpose: "Four-wheeled vehicle with arcade steering, drift and boost.",
        godot_nodes: &["VehicleBody3D", "CollisionShape3D", "MeshInstance3D"],
        properties: &[
            measured("speed", "28.0", "m/s", "5.0", "90.0"),
            measured("acceleration", "12.0", "m/s2", "1.0", "60.0"),
            measured("steer_speed", "2.2", "rad/s", "0.2", "8.0"),
            measured("drift_grip", "0.55", "ratio", "0.05", "1.0"),
        ],
    },
    PresetCard {
        id: "preset.player.top_down_3d",
        title: "Top-down action character",
        purpose: "Eight-way movement with an aim direction under an overhead camera.",
        godot_nodes: &["CharacterBody3D", "CollisionShape3D", "MeshInstance3D"],
        properties: &[
            measured("speed", "8.0", "m/s", "1.0", "30.0"),
            measured("max_health", "100.0", "hp", "1.0", "1000.0"),
            measured("dash_speed", "18.0", "m/s", "2.0", "60.0"),
        ],
    },
    PresetCard {
        id: "preset.player.runner",
        title: "Auto-runner character",
        purpose: "Constant forward motion with lane changes, jump and slide.",
        godot_nodes: &["CharacterBody3D", "CollisionShape3D", "MeshInstance3D"],
        properties: &[
            measured("speed", "14.0", "m/s", "3.0", "60.0"),
            measured("jump_velocity", "8.0", "m/s", "1.0", "25.0"),
            measured("lane_width", "2.5", "m", "0.5", "8.0"),
        ],
    },
    PresetCard {
        id: "preset.player.physics_hand",
        title: "Physics grab hand",
        purpose: "First-person cursor that picks up, carries and throws rigid bodies.",
        godot_nodes: &["CharacterBody3D", "Camera3D", "RayCast3D"],
        properties: &[
            measured("speed", "5.0", "m/s", "1.0", "20.0"),
            measured("grab_range", "3.0", "m", "0.5", "12.0"),
            measured("throw_force", "9.0", "N", "1.0", "60.0"),
        ],
    },
    PresetCard {
        id: "preset.player.build_cursor",
        title: "Build cursor",
        purpose: "Overhead pointer that places, sells and upgrades structures.",
        godot_nodes: &["Node3D", "Camera3D", "RayCast3D"],
        properties: &[
            measured("pan_speed", "12.0", "m/s", "1.0", "50.0"),
            number("starting_gold", "120"),
        ],
    },
    // ---------------------------------------------------------------- camera
    PresetCard {
        id: "preset.camera.follow_3d",
        title: "Follow camera",
        purpose: "Smoothed third-person camera with collision pull-in.",
        godot_nodes: &["Camera3D", "Node3D"],
        properties: &[
            measured("camera_distance", "6.0", "m", "1.0", "30.0"),
            measured("camera_height", "2.4", "m", "0.0", "20.0"),
            measured("follow_lag", "0.12", "s", "0.0", "1.0"),
        ],
    },
    PresetCard {
        id: "preset.camera.chase_3d",
        title: "Chase camera",
        purpose: "Speed-reactive vehicle camera that widens with velocity.",
        godot_nodes: &["Camera3D", "Node3D"],
        properties: &[
            measured("camera_distance", "8.5", "m", "2.0", "30.0"),
            measured("fov_boost", "12.0", "deg", "0.0", "40.0"),
        ],
    },
    PresetCard {
        id: "preset.camera.first_person",
        title: "First-person camera",
        purpose: "Head-mounted camera with view bob and recoil hooks.",
        godot_nodes: &["Camera3D"],
        properties: &[
            measured("look_sensitivity", "0.25", "deg/px", "0.02", "2.0"),
            measured("field_of_view", "75.0", "deg", "50.0", "110.0"),
        ],
    },
    PresetCard {
        id: "preset.camera.top_down_2d",
        title: "Top-down 2D camera",
        purpose: "Overhead camera that follows the player inside level bounds.",
        godot_nodes: &["Camera2D"],
        properties: &[
            measured("zoom", "1.0", "x", "0.2", "4.0"),
            measured("follow_lag", "0.1", "s", "0.0", "1.0"),
        ],
    },
    PresetCard {
        id: "preset.camera.side_scroll_2d",
        title: "Side-scrolling camera",
        purpose: "Look-ahead 2D camera with a dead zone around the player.",
        godot_nodes: &["Camera2D"],
        properties: &[
            measured("zoom", "1.0", "x", "0.2", "4.0"),
            measured("look_ahead", "80.0", "px", "0.0", "400.0"),
        ],
    },
    PresetCard {
        id: "preset.camera.isometric_3d",
        title: "Isometric camera",
        purpose: "Fixed-angle orthographic camera over a 3D scene.",
        godot_nodes: &["Camera3D"],
        properties: &[
            measured("camera_distance", "18.0", "m", "4.0", "60.0"),
            measured("pitch", "45.0", "deg", "15.0", "89.0"),
        ],
    },
    PresetCard {
        id: "preset.camera.orbit_3d",
        title: "Orbit camera",
        purpose: "Player-driven orbit and zoom around a fixed focus point.",
        godot_nodes: &["Camera3D", "Node3D"],
        properties: &[
            measured("camera_distance", "14.0", "m", "2.0", "60.0"),
            measured("orbit_speed", "1.4", "rad/s", "0.1", "6.0"),
        ],
    },
    PresetCard {
        id: "preset.camera.runner_chase",
        title: "Runner chase camera",
        purpose: "Fixed-offset camera behind an auto-running player.",
        godot_nodes: &["Camera3D"],
        properties: &[
            measured("camera_distance", "7.5", "m", "2.0", "25.0"),
            measured("camera_height", "3.0", "m", "0.5", "15.0"),
        ],
    },
    // ---------------------------------------------------------------- level
    PresetCard {
        id: "preset.level.islands",
        title: "Floating islands",
        purpose: "Scattered walkable islands separated by glide-length gaps.",
        godot_nodes: &["Node3D", "StaticBody3D", "CSGBox3D", "NavigationRegion3D"],
        properties: &[
            number("island_count", "7"),
            measured("gap_distance", "12.0", "m", "2.0", "60.0"),
            measured("height_variation", "6.0", "m", "0.0", "40.0"),
        ],
    },
    PresetCard {
        id: "preset.level.arena",
        title: "Combat arena",
        purpose: "Enclosed arena with cover blocks and spawn markers.",
        godot_nodes: &["Node3D", "StaticBody3D", "CSGBox3D", "Marker3D"],
        properties: &[
            measured("arena_radius", "28.0", "m", "8.0", "120.0"),
            number("cover_count", "12"),
        ],
    },
    PresetCard {
        id: "preset.level.track_oval",
        title: "Oval race track",
        purpose: "Closed circuit with barriers, checkpoints and a start grid.",
        godot_nodes: &["Path3D", "StaticBody3D", "CSGBox3D", "Area3D", "Marker3D"],
        properties: &[
            measured("track_length", "900.0", "m", "150.0", "6000.0"),
            measured("track_width", "14.0", "m", "6.0", "40.0"),
            number("checkpoint_count", "6"),
        ],
    },
    PresetCard {
        id: "preset.level.tilemap_dungeon",
        title: "Tilemap dungeon",
        purpose: "Room-and-corridor dungeon built from a tile layer.",
        godot_nodes: &["TileMapLayer", "Node2D", "Area2D"],
        properties: &[
            number("room_count", "9"),
            number("corridor_width", "2"),
            number("seed", "1"),
        ],
    },
    PresetCard {
        id: "preset.level.platform_course_3d",
        title: "3D platform course",
        purpose: "Sequenced jump platforms, moving blocks and a goal ledge.",
        godot_nodes: &["Node3D", "StaticBody3D", "CSGBox3D", "AnimationPlayer"],
        properties: &[
            number("platform_count", "24"),
            measured("gap_distance", "4.5", "m", "1.0", "20.0"),
            number("moving_platform_count", "5"),
        ],
    },
    PresetCard {
        id: "preset.level.platform_course_2d",
        title: "2D platform course",
        purpose: "Side-on tile course with hazards, springs and a flag.",
        godot_nodes: &["TileMapLayer", "Node2D", "Area2D"],
        properties: &[
            number("platform_count", "40"),
            number("hazard_count", "8"),
            number("seed", "1"),
        ],
    },
    PresetCard {
        id: "preset.level.puzzle_room",
        title: "Physics puzzle room",
        purpose: "Sealed room with crates, plates, doors and a reset button.",
        godot_nodes: &["Node3D", "StaticBody3D", "CSGBox3D", "Area3D"],
        properties: &[number("room_count", "6"), number("crate_count", "5")],
    },
    PresetCard {
        id: "preset.level.defense_lanes",
        title: "Defense lanes",
        purpose: "Creep paths from spawn to base with buildable ground slots.",
        godot_nodes: &["Node3D", "Path3D", "StaticBody3D", "Area3D", "Marker3D"],
        properties: &[
            number("lane_count", "2"),
            number("build_slot_count", "18"),
            measured("path_length", "180.0", "m", "40.0", "800.0"),
        ],
    },
    PresetCard {
        id: "preset.level.open_valley",
        title: "Open valley",
        purpose: "Heightmap valley with tree cover, water and resource clusters.",
        godot_nodes: &[
            "Node3D",
            "StaticBody3D",
            "MeshInstance3D",
            "NavigationRegion3D",
        ],
        properties: &[
            measured("valley_size", "400.0", "m", "80.0", "2000.0"),
            measured("tree_density", "0.35", "ratio", "0.0", "1.0"),
            number("seed", "1"),
        ],
    },
    PresetCard {
        id: "preset.level.endless_corridor",
        title: "Endless corridor",
        purpose: "Streaming lane chunks recycled ahead of the runner.",
        godot_nodes: &["Node3D", "StaticBody3D", "CSGBox3D", "Area3D"],
        properties: &[
            number("lane_count", "3"),
            measured("chunk_length", "40.0", "m", "10.0", "200.0"),
            measured("obstacle_density", "0.4", "ratio", "0.0", "1.0"),
        ],
    },
    // ---------------------------------------------------------------- hud
    PresetCard {
        id: "preset.hud.health_score",
        title: "Health and score HUD",
        purpose: "Health bar and score readout anchored to the screen corners.",
        godot_nodes: &["CanvasLayer", "Control", "ProgressBar", "Label"],
        properties: &[boolean("show_score", "true"), number("score_target", "0")],
    },
    PresetCard {
        id: "preset.hud.lives_score",
        title: "Lives and score HUD",
        purpose: "Remaining lives, collected coins and the level timer.",
        godot_nodes: &["CanvasLayer", "Control", "Label"],
        properties: &[number("lives", "3"), boolean("show_timer", "true")],
    },
    PresetCard {
        id: "preset.hud.lap_timer",
        title: "Lap and timer HUD",
        purpose: "Current lap, position and split times.",
        godot_nodes: &["CanvasLayer", "Control", "Label"],
        properties: &[number("lap_count", "3"), boolean("show_position", "true")],
    },
    PresetCard {
        id: "preset.hud.wave_counter",
        title: "Wave counter HUD",
        purpose: "Wave number, base health and build currency.",
        godot_nodes: &["CanvasLayer", "Control", "ProgressBar", "Label"],
        properties: &[number("wave_count", "10"), boolean("show_gold", "true")],
    },
    PresetCard {
        id: "preset.hud.collectible_counter",
        title: "Collectible counter HUD",
        purpose: "Collected-of-target readout with an objective hint line.",
        godot_nodes: &["CanvasLayer", "Control", "Label"],
        properties: &[number("collect_target", "10"), boolean("show_hint", "true")],
    },
    PresetCard {
        id: "preset.hud.ammo_health",
        title: "Ammo and health HUD",
        purpose: "Crosshair, ammo counter, health bar and frag tally.",
        godot_nodes: &[
            "CanvasLayer",
            "Control",
            "ProgressBar",
            "Label",
            "TextureRect",
        ],
        properties: &[number("ammo_capacity", "30"), number("max_health", "100")],
    },
    PresetCard {
        id: "preset.hud.distance_score",
        title: "Distance and score HUD",
        purpose: "Run distance, multiplier and best-run marker.",
        godot_nodes: &["CanvasLayer", "Control", "Label"],
        properties: &[boolean("show_best", "true"), number("score_target", "0")],
    },
    PresetCard {
        id: "preset.hud.survival_meters",
        title: "Survival meters HUD",
        purpose: "Health, hunger, stamina and the clock.",
        godot_nodes: &["CanvasLayer", "Control", "ProgressBar", "Label"],
        properties: &[number("max_health", "100"), boolean("show_clock", "true")],
    },
    PresetCard {
        id: "preset.hud.move_counter",
        title: "Move counter HUD",
        purpose: "Moves taken, par target and a reset button.",
        godot_nodes: &["CanvasLayer", "Control", "Label", "Button"],
        properties: &[number("par_moves", "12"), boolean("show_reset", "true")],
    },
    // ---------------------------------------------------------------- rules
    PresetCard {
        id: "preset.rules.collect_n_to_unlock",
        title: "Collect N to unlock",
        purpose: "Counts collectibles and opens the goal once the target is met.",
        godot_nodes: &["Node3D", "Area3D", "Timer"],
        properties: &[
            number("collect_target", "10"),
            text("unlock_target", "Goal"),
        ],
    },
    PresetCard {
        id: "preset.rules.reach_goal",
        title: "Reach the goal",
        purpose: "Wins when the player enters the goal volume.",
        godot_nodes: &["Area3D", "Node3D"],
        properties: &[boolean("require_all_checkpoints", "false")],
    },
    PresetCard {
        id: "preset.rules.laps",
        title: "Lap race",
        purpose: "Counts ordered checkpoints into laps and ranks finishers.",
        godot_nodes: &["Node3D", "Area3D", "Timer"],
        properties: &[
            number("lap_count", "3"),
            number("checkpoint_count", "6"),
            choice("difficulty", DIFFICULTY, "normal"),
        ],
    },
    PresetCard {
        id: "preset.rules.survive_time",
        title: "Survive the clock",
        purpose: "Wins when the player is alive at the end of the countdown.",
        godot_nodes: &["Timer", "Node3D"],
        properties: &[measured("time_limit", "300.0", "s", "10.0", "7200.0")],
    },
    PresetCard {
        id: "preset.rules.last_one_standing",
        title: "Last one standing",
        purpose: "Wins when every rival actor is eliminated.",
        godot_nodes: &["Node3D", "Timer"],
        properties: &[
            number("lives", "1"),
            choice("difficulty", DIFFICULTY, "normal"),
        ],
    },
    PresetCard {
        id: "preset.rules.frag_limit",
        title: "Frag limit",
        purpose: "First to the score target ends the match.",
        godot_nodes: &["Node3D", "Timer"],
        properties: &[
            number("score_target", "20"),
            measured("time_limit", "600.0", "s", "30.0", "3600.0"),
        ],
    },
    PresetCard {
        id: "preset.rules.defend_base",
        title: "Defend the base",
        purpose: "Loses when base health reaches zero; wins after the last wave.",
        godot_nodes: &["Node3D", "Area3D", "Timer"],
        properties: &[number("wave_count", "10"), number("max_health", "20")],
    },
    PresetCard {
        id: "preset.rules.solve_puzzle",
        title: "Solve every room",
        purpose: "Wins when each room's exit condition is satisfied.",
        godot_nodes: &["Node3D", "Area3D"],
        properties: &[number("room_count", "6"), number("par_moves", "12")],
    },
    PresetCard {
        id: "preset.rules.endless_distance",
        title: "Endless distance",
        purpose: "Scores by distance travelled until the run ends.",
        godot_nodes: &["Node3D", "Timer"],
        properties: &[
            number("lives", "1"),
            measured("speed_ramp", "0.02", "ratio/s", "0.0", "0.5"),
        ],
    },
    // ---------------------------------------------------------------- enemies and actors
    PresetCard {
        id: "preset.enemy.chaser",
        title: "Chasing enemy",
        purpose: "Navigates to the player and damages on contact.",
        godot_nodes: &[
            "CharacterBody3D",
            "NavigationAgent3D",
            "Area3D",
            "MeshInstance3D",
        ],
        properties: &[
            measured("enemy_speed", "4.5", "m/s", "0.5", "25.0"),
            measured("damage", "10.0", "hp", "1.0", "200.0"),
            measured("max_health", "30.0", "hp", "1.0", "500.0"),
        ],
    },
    PresetCard {
        id: "preset.enemy.patroller",
        title: "Patrolling enemy",
        purpose: "Walks a fixed route and chases once it sees the player.",
        godot_nodes: &["CharacterBody3D", "Path3D", "Area3D", "MeshInstance3D"],
        properties: &[
            measured("enemy_speed", "3.0", "m/s", "0.5", "20.0"),
            measured("attack_range", "8.0", "m", "1.0", "60.0"),
        ],
    },
    PresetCard {
        id: "preset.enemy.turret",
        title: "Turret",
        purpose: "Stationary shooter that tracks and fires at targets in range.",
        godot_nodes: &["StaticBody3D", "Area3D", "RayCast3D", "MeshInstance3D"],
        properties: &[
            measured("attack_range", "18.0", "m", "2.0", "120.0"),
            measured("fire_rate", "1.5", "shots/s", "0.1", "20.0"),
            measured("damage", "8.0", "hp", "1.0", "200.0"),
        ],
    },
    PresetCard {
        id: "preset.enemy.wave_spawner",
        title: "Wave spawner",
        purpose: "Releases timed waves of enemies from marked spawn points.",
        godot_nodes: &["Node3D", "Timer", "Marker3D"],
        properties: &[
            number("wave_count", "10"),
            measured("spawn_interval", "2.0", "s", "0.1", "60.0"),
            measured("wave_growth", "1.25", "ratio", "1.0", "4.0"),
        ],
    },
    PresetCard {
        id: "preset.enemy.bot_duelist",
        title: "Duelling bot",
        purpose: "Arena opponent that strafes, takes cover and shoots back.",
        godot_nodes: &[
            "CharacterBody3D",
            "NavigationAgent3D",
            "RayCast3D",
            "MeshInstance3D",
        ],
        properties: &[
            measured("enemy_speed", "6.0", "m/s", "1.0", "25.0"),
            measured("fire_rate", "2.5", "shots/s", "0.1", "20.0"),
            choice("difficulty", DIFFICULTY, "normal"),
        ],
    },
    PresetCard {
        id: "preset.enemy.creep_walker",
        title: "Creep walker",
        purpose: "Follows a defense lane toward the base and ignores the player.",
        godot_nodes: &["CharacterBody3D", "PathFollow3D", "MeshInstance3D"],
        properties: &[
            measured("enemy_speed", "2.4", "m/s", "0.2", "15.0"),
            measured("max_health", "40.0", "hp", "1.0", "2000.0"),
        ],
    },
    PresetCard {
        id: "preset.enemy.night_stalker",
        title: "Night stalker",
        purpose: "Spawns in darkness, hunts by sound and retreats at dawn.",
        godot_nodes: &[
            "CharacterBody3D",
            "NavigationAgent3D",
            "Area3D",
            "MeshInstance3D",
        ],
        properties: &[
            measured("enemy_speed", "3.6", "m/s", "0.5", "20.0"),
            measured("spawn_interval", "18.0", "s", "1.0", "300.0"),
        ],
    },
    PresetCard {
        id: "preset.actor.racer_ai",
        title: "AI racer",
        purpose: "Rival kart that follows the racing line with rubber-banding.",
        godot_nodes: &["VehicleBody3D", "PathFollow3D", "MeshInstance3D"],
        properties: &[
            measured("enemy_speed", "26.0", "m/s", "5.0", "90.0"),
            choice("difficulty", DIFFICULTY, "normal"),
        ],
    },
    PresetCard {
        id: "preset.actor.npc_wanderer",
        title: "Wandering NPC",
        purpose: "Ambient character that strolls the level and can be talked to.",
        godot_nodes: &[
            "CharacterBody3D",
            "NavigationAgent3D",
            "Area3D",
            "MeshInstance3D",
        ],
        properties: &[measured("walk_speed", "1.8", "m/s", "0.2", "8.0")],
    },
    PresetCard {
        id: "preset.tower.gun_turret",
        title: "Buildable gun tower",
        purpose: "Player-placed tower with cost, range, rate and upgrades.",
        godot_nodes: &["StaticBody3D", "Area3D", "MeshInstance3D"],
        properties: &[
            measured("attack_range", "12.0", "m", "2.0", "60.0"),
            measured("fire_rate", "2.0", "shots/s", "0.1", "20.0"),
            number("build_cost", "40"),
        ],
    },
    PresetCard {
        id: "preset.tower.slow_field",
        title: "Buildable slow field",
        purpose: "Player-placed field that slows creeps inside its radius.",
        godot_nodes: &["StaticBody3D", "Area3D", "MeshInstance3D"],
        properties: &[
            measured("attack_range", "8.0", "m", "1.0", "40.0"),
            measured("slow_factor", "0.5", "ratio", "0.05", "0.95"),
            number("build_cost", "30"),
        ],
    },
    // ---------------------------------------------------------------- pickups and props
    PresetCard {
        id: "preset.pickup.collectible",
        title: "Collectible",
        purpose: "Rotating pickup that increments the objective counter.",
        godot_nodes: &["Area3D", "MeshInstance3D", "AudioStreamPlayer3D"],
        properties: &[
            number("collect_target", "10"),
            measured("spin_speed", "1.2", "rad/s", "0.0", "10.0"),
        ],
    },
    PresetCard {
        id: "preset.pickup.powerup",
        title: "Power-up",
        purpose: "Timed buff pickup that respawns after a cooldown.",
        godot_nodes: &["Area3D", "MeshInstance3D", "Timer"],
        properties: &[
            measured("duration", "8.0", "s", "0.5", "120.0"),
            measured("respawn_time", "20.0", "s", "1.0", "600.0"),
        ],
    },
    PresetCard {
        id: "preset.pickup.ammo_crate",
        title: "Ammo crate",
        purpose: "Refills weapon ammunition and respawns on a timer.",
        godot_nodes: &["Area3D", "MeshInstance3D", "Timer"],
        properties: &[
            number("ammo_capacity", "30"),
            measured("respawn_time", "15.0", "s", "1.0", "600.0"),
        ],
    },
    PresetCard {
        id: "preset.pickup.resource_node",
        title: "Resource node",
        purpose: "Harvestable wood, stone or food that regrows over time.",
        godot_nodes: &["StaticBody3D", "Area3D", "MeshInstance3D"],
        properties: &[
            number("yield_amount", "5"),
            measured("respawn_time", "90.0", "s", "5.0", "3600.0"),
        ],
    },
    PresetCard {
        id: "preset.prop.physics_crate",
        title: "Physics crate",
        purpose: "Grabbable rigid body with mass, friction and break threshold.",
        godot_nodes: &["RigidBody3D", "CollisionShape3D", "MeshInstance3D"],
        properties: &[
            measured("mass", "4.0", "kg", "0.1", "500.0"),
            measured("friction", "0.6", "ratio", "0.0", "2.0"),
        ],
    },
    PresetCard {
        id: "preset.prop.pressure_plate",
        title: "Pressure plate",
        purpose: "Opens a door while enough mass rests on it.",
        godot_nodes: &["StaticBody3D", "Area3D", "AnimationPlayer"],
        properties: &[measured("required_mass", "3.0", "kg", "0.1", "200.0")],
    },
    PresetCard {
        id: "preset.obstacle.runner_hazard",
        title: "Runner hazard",
        purpose: "Lane obstacle that ends or damages the run on contact.",
        godot_nodes: &["Area3D", "StaticBody3D", "MeshInstance3D"],
        properties: &[
            measured("obstacle_density", "0.4", "ratio", "0.0", "1.0"),
            measured("damage", "100.0", "hp", "1.0", "1000.0"),
        ],
    },
    // ---------------------------------------------------------------- abilities and systems
    PresetCard {
        id: "preset.ability.glide",
        title: "Glide",
        purpose: "Hold jump in the air to fall slowly for a bounded time.",
        godot_nodes: &["Node3D", "Timer"],
        properties: &[
            measured("glide_time", "3.0", "s", "0.2", "30.0"),
            measured("glide_fall_speed", "1.2", "m/s", "0.1", "10.0"),
        ],
    },
    PresetCard {
        id: "preset.ability.double_jump",
        title: "Double jump",
        purpose: "One extra airborne jump, refreshed on landing.",
        godot_nodes: &["Node3D"],
        properties: &[number("air_jumps", "1")],
    },
    PresetCard {
        id: "preset.ability.dash",
        title: "Dash",
        purpose: "Short burst of speed on a cooldown.",
        godot_nodes: &["Node3D", "Timer"],
        properties: &[
            measured("dash_speed", "18.0", "m/s", "2.0", "60.0"),
            measured("dash_cooldown", "1.2", "s", "0.05", "20.0"),
        ],
    },
    PresetCard {
        id: "preset.ability.sprint",
        title: "Sprint",
        purpose: "Held sprint that drains and regenerates stamina.",
        godot_nodes: &["Node3D"],
        properties: &[
            measured("sprint_multiplier", "1.6", "x", "1.0", "4.0"),
            measured("stamina_max", "100.0", "pt", "1.0", "1000.0"),
        ],
    },
    PresetCard {
        id: "preset.ability.shoot_hitscan",
        title: "Hitscan weapon",
        purpose: "Instant-hit weapon with ammo, spread and reload.",
        godot_nodes: &["Node3D", "RayCast3D", "Timer", "AudioStreamPlayer3D"],
        properties: &[
            measured("fire_rate", "8.0", "shots/s", "0.2", "30.0"),
            measured("damage", "12.0", "hp", "1.0", "200.0"),
            number("ammo_capacity", "30"),
        ],
    },
    PresetCard {
        id: "preset.ability.melee_swing",
        title: "Melee swing",
        purpose: "Short-range arc attack with wind-up and cooldown.",
        godot_nodes: &["Node3D", "Area3D", "Timer"],
        properties: &[
            measured("damage", "18.0", "hp", "1.0", "200.0"),
            measured("attack_range", "2.0", "m", "0.5", "8.0"),
        ],
    },
    PresetCard {
        id: "preset.ability.drift",
        title: "Drift",
        purpose: "Handbrake slide that charges a boost while held.",
        godot_nodes: &["Node3D", "Timer"],
        properties: &[
            measured("drift_grip", "0.55", "ratio", "0.05", "1.0"),
            measured("boost_power", "9.0", "m/s", "0.0", "40.0"),
        ],
    },
    PresetCard {
        id: "preset.ability.grab_throw",
        title: "Grab and throw",
        purpose: "Pick up a rigid body, carry it and throw it.",
        godot_nodes: &["Node3D", "RayCast3D"],
        properties: &[
            measured("grab_range", "3.0", "m", "0.5", "12.0"),
            measured("throw_force", "9.0", "N", "1.0", "60.0"),
        ],
    },
    PresetCard {
        id: "preset.ability.build_tower",
        title: "Build tower",
        purpose: "Spend currency to place, upgrade or sell a tower on a slot.",
        godot_nodes: &["Node3D", "Area3D"],
        properties: &[
            number("build_cost", "40"),
            measured("sell_refund", "0.6", "ratio", "0.0", "1.0"),
        ],
    },
    PresetCard {
        id: "preset.system.lives",
        title: "Lives",
        purpose: "Finite retries that end the run at zero.",
        godot_nodes: &["Node3D"],
        properties: &[number("lives", "3")],
    },
    PresetCard {
        id: "preset.system.checkpoints",
        title: "Checkpoints",
        purpose: "Respawn markers that restore progress on death.",
        godot_nodes: &["Node3D", "Area3D", "Marker3D"],
        properties: &[
            number("checkpoint_count", "6"),
            measured("respawn_time", "1.0", "s", "0.0", "10.0"),
        ],
    },
    PresetCard {
        id: "preset.system.hunger_stamina",
        title: "Hunger and stamina",
        purpose: "Draining survival meters fed by food and rest.",
        godot_nodes: &["Node3D", "Timer"],
        properties: &[
            measured("hunger_rate", "0.6", "pt/s", "0.0", "10.0"),
            measured("stamina_max", "100.0", "pt", "1.0", "1000.0"),
        ],
    },
    PresetCard {
        id: "preset.system.crafting_basic",
        title: "Basic crafting",
        purpose: "Recipe table turning gathered resources into tools.",
        godot_nodes: &["Node3D", "CanvasLayer", "Control"],
        properties: &[number("recipe_count", "8")],
    },
    PresetCard {
        id: "preset.system.day_night",
        title: "Day and night cycle",
        purpose: "Rotating sun with a dawn/dusk colour ramp.",
        godot_nodes: &["DirectionalLight3D", "WorldEnvironment", "AnimationPlayer"],
        properties: &[
            measured("day_length", "600.0", "s", "30.0", "7200.0"),
            boolean("day_night_enabled", "true"),
        ],
    },
    PresetCard {
        id: "preset.system.score_multiplier",
        title: "Score multiplier",
        purpose: "Streak multiplier that decays when the streak breaks.",
        godot_nodes: &["Node3D", "Timer"],
        properties: &[
            measured("multiplier_step", "0.25", "x", "0.01", "5.0"),
            measured("decay_time", "3.0", "s", "0.1", "60.0"),
        ],
    },
    // ---------------------------------------------------------------- world dressing
    PresetCard {
        id: "preset.light.sun_sky",
        title: "Sun and sky",
        purpose: "Directional sun with a sky environment and ambient fill.",
        godot_nodes: &["DirectionalLight3D", "WorldEnvironment"],
        properties: &[
            measured("light_energy", "1.0", "ratio", "0.0", "16.0"),
            colour("sky_tint", "#9ec8ff"),
        ],
    },
    PresetCard {
        id: "preset.light.dungeon_torches",
        title: "Dungeon torches",
        purpose: "Flickering point lights with a dark ambient floor.",
        godot_nodes: &["OmniLight3D", "WorldEnvironment", "GPUParticles3D"],
        properties: &[
            measured("light_energy", "1.6", "ratio", "0.0", "16.0"),
            measured("flicker_rate", "3.0", "Hz", "0.0", "20.0"),
        ],
    },
    PresetCard {
        id: "preset.weather.rain",
        title: "Rain",
        purpose: "Particle rain with wet ambience and a density knob.",
        godot_nodes: &["GPUParticles3D", "WorldEnvironment", "AudioStreamPlayer3D"],
        properties: &[
            measured("rain_amount", "0.5", "ratio", "0.0", "1.0"),
            boolean("rain_enabled", "true"),
        ],
    },
    PresetCard {
        id: "preset.weather.fog",
        title: "Fog",
        purpose: "Distance fog with density and colour controls.",
        godot_nodes: &["WorldEnvironment"],
        properties: &[
            measured("fog_density", "0.02", "ratio", "0.0", "1.0"),
            boolean("fog_enabled", "true"),
        ],
    },
    PresetCard {
        id: "preset.audio.ambient_loop",
        title: "Ambient loop",
        purpose: "Looping background ambience bed.",
        godot_nodes: &["AudioStreamPlayer"],
        properties: &[measured("volume_db", "-12.0", "dB", "-60.0", "6.0")],
    },
    PresetCard {
        id: "preset.audio.music_track",
        title: "Music track",
        purpose: "Looping music with an intensity crossfade.",
        godot_nodes: &["AudioStreamPlayer"],
        properties: &[
            measured("volume_db", "-8.0", "dB", "-60.0", "6.0"),
            measured("crossfade_time", "2.0", "s", "0.0", "20.0"),
        ],
    },
    PresetCard {
        id: "preset.audio.sfx_bank",
        title: "SFX bank",
        purpose: "Positional sound bank for jumps, hits and pickups.",
        godot_nodes: &["AudioStreamPlayer3D"],
        properties: &[measured("volume_db", "-6.0", "dB", "-60.0", "6.0")],
    },
    PresetCard {
        id: "preset.fx.dust_trail",
        title: "Dust trail",
        purpose: "Ground dust emitted while moving fast.",
        godot_nodes: &["GPUParticles3D"],
        properties: &[measured("emission_rate", "40.0", "p/s", "0.0", "500.0")],
    },
    PresetCard {
        id: "preset.fx.impact_sparks",
        title: "Impact sparks",
        purpose: "One-shot sparks on hits and collisions.",
        godot_nodes: &["GPUParticles3D"],
        properties: &[measured("emission_rate", "80.0", "p/s", "0.0", "500.0")],
    },
];

/// Every preset the compiler may name, in authored order.
#[must_use]
pub fn presets() -> &'static [PresetCard] {
    PRESETS
}

/// The card for `id`, or `None` when the id is not in the catalogue.
#[must_use]
pub fn preset(id: &str) -> Option<&'static PresetCard> {
    PRESETS.iter().find(|card| card.id == id)
}

/// Whether `id` names a Godot class the compiler may address.
#[must_use]
pub fn is_godot_class(id: &str) -> bool {
    GODOT_CLASSES.contains(&id)
}

/// Whether `id` is a legal capability id: a catalogued preset or a known Godot class.
#[must_use]
pub fn is_known_id(id: &str) -> bool {
    preset(id).is_some() || is_godot_class(id)
}

/// The `<domain>` segment of a preset id (`preset.player.fps` -> `player`).
#[must_use]
pub fn preset_domain(id: &str) -> Option<&str> {
    id.strip_prefix(PRESET_PREFIX)
        .and_then(|rest| rest.split('.').next())
        .filter(|domain| !domain.is_empty())
}

/// A property on a preset, if the catalogue declares it.
#[must_use]
pub fn preset_property(preset_id: &str, property: &str) -> Option<&'static PropertySpec> {
    preset(preset_id).and_then(|card| card.properties.iter().find(|spec| spec.name == property))
}

/// The presets that expose `property`, in catalogue order.
#[must_use]
pub fn presets_exposing(property: &str) -> Vec<&'static PresetCard> {
    PRESETS
        .iter()
        .filter(|card| card.properties.iter().any(|spec| spec.name == property))
        .collect()
}

/// One row of the fast-path noun table: the words a person uses for a knob, and the knob.
///
/// `node_class` narrows the search when the property lives on a Godot node rather than on a
/// Bhippi script (`brightness` is `DirectionalLight3D.light_energy`, not a script var).
/// `bool_property` is the on/off form of the same noun, and `choice` is the closed option
/// set when the knob is categorical.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NounEntry {
    pub words: &'static [&'static str],
    pub property: &'static str,
    pub node_class: Option<&'static str>,
    pub bool_property: Option<&'static str>,
    pub choice: Option<&'static [&'static str]>,
}

const fn noun(words: &'static [&'static str], property: &'static str) -> NounEntry {
    NounEntry {
        words,
        property,
        node_class: None,
        bool_property: None,
        choice: None,
    }
}

const fn class_noun(
    words: &'static [&'static str],
    property: &'static str,
    node_class: &'static str,
) -> NounEntry {
    NounEntry {
        words,
        property,
        node_class: Some(node_class),
        bool_property: None,
        choice: None,
    }
}

const NOUNS: &[NounEntry] = &[
    noun(
        &["jump height", "jump power", "jump", "jumping"],
        "jump_velocity",
    ),
    noun(
        &[
            "move speed",
            "movement speed",
            "walk speed",
            "speed",
            "run",
            "running",
        ],
        "speed",
    ),
    noun(&["glide time", "gliding", "glide"], "glide_time"),
    noun(&["gravity"], "gravity"),
    noun(&["acceleration", "accel"], "acceleration"),
    noun(&["dash"], "dash_speed"),
    noun(&["sprint"], "sprint_multiplier"),
    noun(&["stamina"], "stamina_max"),
    noun(&["hunger"], "hunger_rate"),
    noun(&["enemy speed", "enemies", "enemy"], "enemy_speed"),
    noun(&["spawn rate", "spawn interval", "spawn"], "spawn_interval"),
    noun(&["waves", "wave count", "wave"], "wave_count"),
    noun(&["laps", "lap count", "lap"], "lap_count"),
    noun(&["lives", "life"], "lives"),
    noun(&["checkpoints", "checkpoint"], "checkpoint_count"),
    noun(&["timer", "time limit", "countdown"], "time_limit"),
    noun(&["health", "hp", "hit points"], "max_health"),
    noun(&["damage", "hit damage"], "damage"),
    noun(&["fire rate", "rate of fire"], "fire_rate"),
    noun(&["ammo", "ammunition", "magazine"], "ammo_capacity"),
    noun(
        &["range", "attack range", "tower range", "turret range"],
        "attack_range",
    ),
    noun(&["score target", "target score", "score"], "score_target"),
    noun(
        &[
            "collectibles",
            "collectable",
            "feathers",
            "coins",
            "collect target",
        ],
        "collect_target",
    ),
    noun(
        &["obstacles", "obstacle density", "hazards"],
        "obstacle_density",
    ),
    noun(&["boost"], "boost_power"),
    noun(&["drift", "grip"], "drift_grip"),
    noun(&["steering", "steer", "turning"], "steer_speed"),
    noun(&["camera distance", "camera", "zoom"], "camera_distance"),
    noun(
        &["sensitivity", "look sensitivity", "mouse sensitivity"],
        "look_sensitivity",
    ),
    noun(&["mass", "weight"], "mass"),
    noun(&["friction", "grippiness"], "friction"),
    noun(&["throw force", "throw"], "throw_force"),
    noun(&["grab range", "grab", "reach"], "grab_range"),
    noun(&["build cost", "tower cost", "cost"], "build_cost"),
    noun(&["par", "par moves", "move limit"], "par_moves"),
    noun(
        &["day length", "day night cycle", "day", "night"],
        "day_length",
    ),
    class_noun(
        &["brightness", "sunlight", "sun", "light", "lighting"],
        "light_energy",
        "DirectionalLight3D",
    ),
    class_noun(
        &["fog", "fog density", "haze"],
        "fog_density",
        "WorldEnvironment",
    ),
    class_noun(
        &["volume", "music", "loudness"],
        "volume_db",
        "AudioStreamPlayer3D",
    ),
    noun(&["rain", "rainfall"], "rain_amount"),
    noun(
        &["particles", "emission rate", "sparks", "dust"],
        "emission_rate",
    ),
    NounEntry {
        words: &["difficulty"],
        property: "difficulty",
        node_class: None,
        bool_property: None,
        choice: Some(DIFFICULTY),
    },
];

/// The fast-path noun table. Longest phrase wins, so `jump height` beats `jump`.
#[must_use]
pub fn nouns() -> &'static [NounEntry] {
    NOUNS
}

impl NounEntry {
    /// The boolean twin of this knob, used when the utterance says "on" or "off".
    #[must_use]
    pub fn toggle_property(&self) -> Option<&'static str> {
        self.bool_property.or(match self.property {
            "rain_amount" => Some("rain_enabled"),
            "fog_density" => Some("fog_enabled"),
            "day_length" => Some("day_night_enabled"),
            _ => None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{
        is_known_id, nouns, preset, preset_domain, preset_property, presets, presets_exposing,
        PropertyKind, GODOT_CLASSES, PRESET_PREFIX,
    };
    use std::collections::BTreeSet;

    #[test]
    fn preset_ids_are_unique_sorted_shaped_and_documented() {
        let mut seen = BTreeSet::new();
        for card in presets() {
            assert!(seen.insert(card.id), "duplicate preset id {}", card.id);
            assert!(
                card.id.starts_with(PRESET_PREFIX),
                "{} lacks the preset prefix",
                card.id
            );
            assert_eq!(
                card.id.split('.').count(),
                3,
                "{} is not preset.<domain>.<name>",
                card.id
            );
            assert!(!card.title.is_empty() && !card.purpose.is_empty());
            assert!(!card.godot_nodes.is_empty(), "{} builds nothing", card.id);
            assert!(!card.properties.is_empty(), "{} exposes nothing", card.id);
        }
        assert!(presets().len() > 50);
    }

    #[test]
    fn every_preset_only_builds_catalogued_godot_classes() {
        for card in presets() {
            for node in card.godot_nodes {
                assert!(
                    GODOT_CLASSES.contains(node),
                    "{} names unknown class {node}",
                    card.id
                );
            }
        }
    }

    #[test]
    fn property_names_are_unique_per_preset_and_choices_default_inside_their_options() {
        for card in presets() {
            let mut seen = BTreeSet::new();
            for spec in card.properties {
                assert!(seen.insert(spec.name), "{} repeats {}", card.id, spec.name);
                assert!(!spec.default.is_empty());
                if let PropertyKind::Choice(options) = spec.kind {
                    assert!(options.len() >= 2);
                    assert!(options.contains(&spec.default));
                }
            }
        }
    }

    #[test]
    fn godot_classes_are_unique_and_sorted() {
        let sorted = {
            let mut copy = GODOT_CLASSES.to_vec();
            copy.sort_unstable();
            copy.dedup();
            copy
        };
        assert_eq!(sorted, GODOT_CLASSES.to_vec());
    }

    #[test]
    fn lookups_answer_for_known_ids_only() {
        assert!(is_known_id("preset.ability.glide"));
        assert!(is_known_id("CharacterBody3D"));
        assert!(!is_known_id("preset.ability.teleport"));
        assert!(!is_known_id("CharacterBody4D"));
        assert_eq!(preset_domain("preset.player.fps"), Some("player"));
        assert_eq!(preset_domain("CharacterBody3D"), None);
        assert!(preset("preset.ability.glide").is_some());
        assert!(preset_property("preset.ability.glide", "glide_time").is_some());
        assert!(preset_property("preset.ability.glide", "glide_range").is_none());
    }

    #[test]
    fn every_noun_points_at_a_property_some_preset_actually_exposes() {
        let mut seen = BTreeSet::new();
        for entry in nouns() {
            assert!(!entry.words.is_empty());
            for word in entry.words {
                assert!(seen.insert(*word), "{word} appears in the noun table twice");
                assert_eq!(*word, word.to_lowercase(), "{word} is not lowercase");
            }
            assert!(
                !presets_exposing(entry.property).is_empty(),
                "no preset exposes {}",
                entry.property
            );
        }
    }
}
