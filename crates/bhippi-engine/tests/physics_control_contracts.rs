#![allow(clippy::expect_used, clippy::unwrap_used)]

use bhippi_engine::control_contract::{
    AccessibilityInputContract, CameraBlendContract, CameraBlendCurve, CameraCollisionContract,
    CameraModifierContract, CameraRigContract, CameraRigKind, CharacterModesContract,
    CharacterMovementContract, CharacterPresetContract, CharacterPresetKind, ContextBinding,
    ControlContractSet, DeviceSupportContract, InputContextContract, InputControlContract,
    InputDeviceClass, InputTrigger, RebindConflictPolicy, RebindingContract,
    CONTROL_CONTRACT_FORMAT,
};
use bhippi_engine::input::InputDocument;
use bhippi_engine::physics_contract::{
    BodyContract, BodyKind, ColliderContract, ColliderShapeContract, CollisionDetection,
    CollisionLayerContract, ConstraintContract, ConstraintKindContract, PhysicsLaneContract,
    PhysicsMaterialContract, PhysicsQueryContract, PhysicsQueryFilter, PHYSICS_CONTRACT_FORMAT,
};
use bhippi_engine::registry::CapabilityRegistry;
use bhippi_engine::runtime_contract::RuntimeEntityHandle;

fn registry() -> CapabilityRegistry {
    CapabilityRegistry::core().expect("registry")
}

fn physics_lane() -> PhysicsLaneContract {
    PhysicsLaneContract {
        format: PHYSICS_CONTRACT_FORMAT.to_owned(),
        capability_ids: vec![
            "component.rigid_body".to_owned(),
            "component.collider".to_owned(),
        ],
        step_micros: 16_667,
        maximum_substeps: 4,
        position_tolerance: 0.001,
        velocity_tolerance: 0.001,
        cpu_micros_per_step: 4_000,
        resident_bytes: 64 * 1024 * 1024,
        materials: vec![PhysicsMaterialContract {
            id: "material.default".to_owned(),
            friction: 0.6,
            restitution: 0.1,
            density: 1.0,
        }],
        layers: vec![
            CollisionLayerContract {
                id: "layer.world".to_owned(),
                bit: 0,
            },
            CollisionLayerContract {
                id: "layer.player".to_owned(),
                bit: 1,
            },
        ],
    }
}

fn entity(id: u64) -> RuntimeEntityHandle {
    RuntimeEntityHandle { id, generation: 1 }
}

fn body() -> BodyContract {
    BodyContract {
        entity: entity(1),
        kind: BodyKind::Dynamic,
        mass: 70.0,
        linear_damping: 0.1,
        angular_damping: 0.2,
        gravity_scale: 1.0,
        collision_detection: CollisionDetection::Continuous,
        colliders: vec![ColliderContract {
            id: "collider.player".to_owned(),
            shape: ColliderShapeContract::Capsule {
                radius: 0.35,
                half_height: 0.9,
            },
            material: "material.default".to_owned(),
            layer: "layer.player".to_owned(),
            collides_with: vec!["layer.world".to_owned()],
            sensor: false,
        }],
    }
}

#[test]
fn physics_contracts_bind_to_registered_components_and_validate_shapes() {
    let lane = physics_lane();
    lane.validate(&registry()).expect("lane valid");
    lane.validate_body(&body()).expect("body valid");

    let query = PhysicsQueryContract::Raycast {
        origin: [0.0, 1.0, 0.0],
        direction: [0.0, -1.0, 0.0],
        max_distance: 2.0,
        filter: PhysicsQueryFilter {
            layers: vec!["layer.world".to_owned()],
            exclude: vec![entity(1)],
            include_sensors: false,
        },
    };
    query.validate(&lane).expect("query valid");
}

#[test]
fn physics_invalid_mass_layers_queries_and_constraints_fail_closed() {
    let lane = physics_lane();
    let mut invalid_body = body();
    invalid_body.mass = 0.0;
    assert!(lane.validate_body(&invalid_body).is_err());

    let mut invalid_body = body();
    invalid_body.colliders[0].layer = "layer.ghost".to_owned();
    assert!(lane.validate_body(&invalid_body).is_err());

    let query = PhysicsQueryContract::Raycast {
        origin: [0.0; 3],
        direction: [0.0; 3],
        max_distance: 0.0,
        filter: PhysicsQueryFilter {
            layers: vec!["layer.ghost".to_owned()],
            exclude: Vec::new(),
            include_sensors: false,
        },
    };
    assert!(query.validate(&lane).is_err());

    let same_endpoint = ConstraintContract {
        id: "constraint.hinge".to_owned(),
        first: entity(1),
        second: entity(1),
        kind: ConstraintKindContract::Hinge {
            axis: [0.0, 1.0, 0.0],
            limits: Some([-1.0, 1.0]),
        },
        break_force: None,
    };
    assert!(same_endpoint.validate().is_err());
}

fn contexts() -> Vec<InputContextContract> {
    vec![InputContextContract {
        id: "input.gameplay".to_owned(),
        priority: 10,
        enabled_by_default: true,
        blocks_lower_contexts: false,
        bindings: vec![
            ContextBinding {
                action: "jump".to_owned(),
                trigger: InputTrigger::Pressed,
                chord: vec!["Space".to_owned()],
            },
            ContextBinding {
                action: "fire".to_owned(),
                trigger: InputTrigger::Held,
                chord: vec!["ShiftLeft".to_owned(), "Mouse0".to_owned()],
            },
            ContextBinding {
                action: "move_x".to_owned(),
                trigger: InputTrigger::Axis,
                chord: vec!["AxisLeftX".to_owned()],
            },
        ],
    }]
}

fn camera(kind: CameraRigKind, id: &str) -> CameraRigContract {
    CameraRigContract {
        id: id.to_owned(),
        kind,
        offset: [0.0, 2.0, -4.0],
        fov_radians: 1.0,
        tracking_strength: 0.8,
        blend: CameraBlendContract {
            duration_millis: 250,
            curve: CameraBlendCurve::SmoothStep,
            preserve_target: true,
        },
        collision: CameraCollisionContract {
            enabled: true,
            probe_radius: 0.2,
            minimum_distance: 0.3,
            restore_speed: 5.0,
            occlusion_fade: true,
        },
        modifiers: CameraModifierContract {
            position_damping: 0.15,
            rotation_damping: 0.1,
            maximum_shake: 0.4,
        },
        capability_ids: vec!["component.camera".to_owned()],
    }
}

fn character(kind: CharacterPresetKind, id: &str, camera_rig: &str) -> CharacterPresetContract {
    CharacterPresetContract {
        id: id.to_owned(),
        kind,
        movement: CharacterMovementContract {
            move_speed: 5.0,
            acceleration: 12.0,
            braking: 10.0,
            jump_speed: 5.5,
            air_control: 0.35,
            max_slope_radians: 0.7,
            step_height: 0.3,
        },
        modes: CharacterModesContract {
            crouch_height: Some(1.1),
            slide_speed: Some(7.0),
            swim_speed: Some(3.0),
            climb_speed: Some(2.0),
            ladder_speed: Some(2.0),
            mantle_height: Some(1.2),
            root_motion: false,
        },
        input_context: "input.gameplay".to_owned(),
        camera_rig: camera_rig.to_owned(),
        capability_ids: vec!["component.character_controller".to_owned()],
    }
}

fn control_set(registry: &CapabilityRegistry) -> ControlContractSet {
    let camera_specs = [
        (CameraRigKind::FirstPerson, "camera.first_person"),
        (CameraRigKind::ThirdPerson, "camera.third_person"),
        (CameraRigKind::Platformer, "camera.platformer"),
        (CameraRigKind::TopDown, "camera.top_down"),
        (CameraRigKind::Racing, "camera.racing"),
    ];
    let character_specs = [
        (
            CharacterPresetKind::FirstPerson,
            "character.first_person",
            "camera.first_person",
        ),
        (
            CharacterPresetKind::ThirdPerson,
            "character.third_person",
            "camera.third_person",
        ),
        (
            CharacterPresetKind::Platformer,
            "character.platformer",
            "camera.platformer",
        ),
        (
            CharacterPresetKind::TopDown,
            "character.top_down",
            "camera.top_down",
        ),
        (
            CharacterPresetKind::Flying,
            "character.flying",
            "camera.racing",
        ),
    ];
    ControlContractSet {
        format: CONTROL_CONTRACT_FORMAT.to_owned(),
        capability_registry_hash: registry.hash.clone(),
        input: InputControlContract {
            contexts: contexts(),
            rebinding: RebindingContract {
                enabled: true,
                conflict_policy: RebindConflictPolicy::Reject,
                reserved_codes: vec!["Escape".to_owned()],
                persist_profile: true,
            },
            accessibility: AccessibilityInputContract {
                hold_to_toggle_supported: true,
                simultaneous_chord_alternative_required: true,
                stick_deadzone: 0.15,
                pointer_sensitivity: 1.0,
                vibration_scale: 0.5,
            },
            devices: vec![
                DeviceSupportContract {
                    device: InputDeviceClass::KeyboardMouse,
                    supported: false,
                    evidence: None,
                    limitation: Some("device evidence is outside this contract slice".to_owned()),
                },
                DeviceSupportContract {
                    device: InputDeviceClass::Gamepad,
                    supported: false,
                    evidence: None,
                    limitation: Some("device backend not integrated".to_owned()),
                },
                DeviceSupportContract {
                    device: InputDeviceClass::Touch,
                    supported: false,
                    evidence: None,
                    limitation: Some("device backend not integrated".to_owned()),
                },
            ],
        },
        characters: character_specs
            .into_iter()
            .map(|(kind, id, rig)| character(kind, id, rig))
            .collect(),
        cameras: camera_specs
            .into_iter()
            .map(|(kind, id)| camera(kind, id))
            .collect(),
    }
}

#[test]
fn every_character_preset_input_context_and_camera_contract_validates_deterministically() {
    let registry = registry();
    let controls = control_set(&registry);
    controls
        .validate(&InputDocument::default(), &registry)
        .expect("control contract valid");
    assert_eq!(controls.characters.len(), CharacterPresetKind::ALL.len());
    assert!(controls.input.contexts[0].bindings[1].chord.len() > 1);
}

#[test]
fn stale_registry_unknown_bindings_and_incomplete_device_truth_fail_closed() {
    let registry = registry();
    let mut controls = control_set(&registry);
    controls.capability_registry_hash = "stale".to_owned();
    assert!(controls
        .validate(&InputDocument::default(), &registry)
        .is_err());

    let mut controls = control_set(&registry);
    controls.input.contexts[0].bindings[0].action = "teleport".to_owned();
    assert!(controls
        .validate(&InputDocument::default(), &registry)
        .is_err());

    let mut controls = control_set(&registry);
    controls.input.devices.pop();
    assert!(controls
        .validate(&InputDocument::default(), &registry)
        .is_err());
}

#[test]
fn invalid_character_and_camera_parameters_are_rejected_without_backend_guessing() {
    let registry = registry();
    let mut controls = control_set(&registry);
    controls.characters[0].movement.air_control = 2.0;
    assert!(controls
        .validate(&InputDocument::default(), &registry)
        .is_err());

    let mut controls = control_set(&registry);
    controls.cameras[0].collision.probe_radius = 0.0;
    assert!(controls
        .validate(&InputDocument::default(), &registry)
        .is_err());

    let mut controls = control_set(&registry);
    controls.cameras[0].blend.duration_millis = 0;
    assert!(controls
        .validate(&InputDocument::default(), &registry)
        .is_err());
}
