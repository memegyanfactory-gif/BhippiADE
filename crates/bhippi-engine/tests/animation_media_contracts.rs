#![allow(clippy::expect_used, clippy::unwrap_used)]

use bhippi_engine::animation_contract::{
    AnimationBudgetContract, AnimationClipContract, AnimationContractSet, AnimationEventContract,
    AnimationGraphContract, AnimationLayerContract, AnimationNodeContract, AnimationNodeKind,
    BoneContract, BoneTrackContract, CompressionContract, IkConstraintContract, IkSolverKind,
    RetargetContract, SkeletonContract, TransformKey, ANIMATION_CONTRACT_FORMAT,
};
use bhippi_engine::media_contract::{
    AttenuationContract, AudioBudgetContract, AudioClipContract, AudioContract, AudioDeviceState,
    AudioEffectContract, AudioEffectKind, AudioEventAction, AudioEventContract, CurvePoint,
    MediaContractSet, MixerBusContract, ReverbZoneContract, SpatialAudioContract,
    VfxBudgetContract, VfxEdgeContract, VfxExecutionClass, VfxGraphContract, VfxLodContract,
    VfxNodeContract, VfxNodeKind, MEDIA_CONTRACT_FORMAT,
};
use bhippi_engine::registry::CapabilityRegistry;
use bhippi_engine::runtime_contract::{RuntimeEntityHandle, RuntimeResourceHandle};
use std::collections::BTreeMap;

fn registry() -> CapabilityRegistry {
    CapabilityRegistry::core().expect("registry")
}

fn identity() -> [f32; 16] {
    [
        1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
    ]
}

fn skeleton() -> SkeletonContract {
    SkeletonContract {
        id: "skeleton.humanoid".to_owned(),
        bones: vec![
            BoneContract {
                id: "bone.hip".to_owned(),
                index: 0,
                parent: None,
                inverse_bind: identity(),
            },
            BoneContract {
                id: "bone.knee".to_owned(),
                index: 1,
                parent: Some("bone.hip".to_owned()),
                inverse_bind: identity(),
            },
            BoneContract {
                id: "bone.foot".to_owned(),
                index: 2,
                parent: Some("bone.knee".to_owned()),
                inverse_bind: identity(),
            },
        ],
    }
}

fn key(time_seconds: f32) -> TransformKey {
    TransformKey {
        time_seconds,
        translation: [0.0, 0.0, 0.0],
        rotation: [0.0, 0.0, 0.0, 1.0],
        scale: [1.0, 1.0, 1.0],
    }
}

fn clip() -> AnimationClipContract {
    AnimationClipContract {
        id: "clip.walk".to_owned(),
        skeleton: "skeleton.humanoid".to_owned(),
        duration_seconds: 1.0,
        looping: true,
        root_motion_bone: Some("bone.hip".to_owned()),
        tracks: vec![BoneTrackContract {
            bone: "bone.hip".to_owned(),
            keys: vec![key(0.0), key(1.0)],
        }],
        events: vec![AnimationEventContract {
            id: "event.footstep".to_owned(),
            time_seconds: 0.5,
            payload_schema: Some("bhippi-footstep@1".to_owned()),
        }],
        compression: CompressionContract {
            translation_error: 0.001,
            rotation_error_radians: 0.001,
            scale_error: 0.001,
        },
    }
}

fn animation_set(registry: &CapabilityRegistry) -> AnimationContractSet {
    AnimationContractSet {
        format: ANIMATION_CONTRACT_FORMAT.to_owned(),
        capability_registry_hash: registry.hash.clone(),
        capability_ids: vec!["component.animation_player".to_owned()],
        skeletons: vec![skeleton()],
        clips: vec![clip()],
        graphs: vec![AnimationGraphContract {
            id: "graph.locomotion".to_owned(),
            parameters: BTreeMap::from([("speed".to_owned(), 0.0)]),
            nodes: vec![
                AnimationNodeContract {
                    id: "node.walk".to_owned(),
                    kind: AnimationNodeKind::Clip {
                        clip: "clip.walk".to_owned(),
                    },
                },
                AnimationNodeContract {
                    id: "node.cached_walk".to_owned(),
                    kind: AnimationNodeKind::PoseCache {
                        source: "node.walk".to_owned(),
                    },
                },
            ],
            transitions: Vec::new(),
            layers: vec![AnimationLayerContract {
                id: "layer.base".to_owned(),
                entry_node: "node.cached_walk".to_owned(),
                weight: 1.0,
                additive: false,
                bone_mask: vec!["bone.hip".to_owned()],
            }],
        }],
        constraints: vec![IkConstraintContract {
            id: "ik.left_leg".to_owned(),
            solver: IkSolverKind::TwoBone,
            chain: vec![
                "bone.hip".to_owned(),
                "bone.knee".to_owned(),
                "bone.foot".to_owned(),
            ],
            target: [0.0, 0.0, 0.0],
            pole_target: Some([0.0, 0.0, 1.0]),
            weight: 1.0,
            iterations: 8,
            tolerance: 0.001,
        }],
        retargets: vec![RetargetContract {
            source_skeleton: "skeleton.humanoid".to_owned(),
            target_skeleton: "skeleton.humanoid".to_owned(),
            bone_map: BTreeMap::from([
                ("bone.hip".to_owned(), "bone.hip".to_owned()),
                ("bone.knee".to_owned(), "bone.knee".to_owned()),
                ("bone.foot".to_owned(), "bone.foot".to_owned()),
            ]),
        }],
        budgets: AnimationBudgetContract {
            maximum_characters: 100,
            cpu_micros_per_frame: 4_000,
            pose_cache_bytes: 16 * 1024 * 1024,
        },
    }
}

#[test]
fn skeleton_clip_graph_ik_and_retarget_contracts_validate_together() {
    let registry = registry();
    animation_set(&registry)
        .validate(&registry)
        .expect("animation contracts valid");
}

#[test]
fn animation_cycles_dangling_tracks_unordered_keys_and_bad_ik_fail_closed() {
    let registry = registry();

    let mut contracts = animation_set(&registry);
    contracts.skeletons[0].bones[0].parent = Some("bone.foot".to_owned());
    assert!(contracts.validate(&registry).is_err());

    let mut contracts = animation_set(&registry);
    contracts.clips[0].tracks[0].bone = "bone.missing".to_owned();
    assert!(contracts.validate(&registry).is_err());

    let mut contracts = animation_set(&registry);
    contracts.clips[0].tracks[0].keys = vec![key(0.8), key(0.2)];
    assert!(contracts.validate(&registry).is_err());

    let mut contracts = animation_set(&registry);
    contracts.constraints[0].chain.pop();
    assert!(contracts.validate(&registry).is_err());
}

fn curve() -> Vec<CurvePoint> {
    vec![
        CurvePoint {
            time: 0.0,
            value: 0.0,
        },
        CurvePoint {
            time: 1.0,
            value: 1.0,
        },
    ]
}

fn media_set(registry: &CapabilityRegistry) -> MediaContractSet {
    let smoke = VfxGraphContract {
        id: "vfx.smoke".to_owned(),
        nodes: vec![VfxNodeContract {
            id: "node.smoke_emitter".to_owned(),
            execution: VfxExecutionClass::Cpu,
            kind: VfxNodeKind::Emitter {
                rate: 20.0,
                burst: 0,
            },
        }],
        edges: Vec::new(),
        lod: vec![VfxLodContract {
            distance: 0.0,
            spawn_scale: 1.0,
        }],
        budgets: VfxBudgetContract {
            maximum_live_particles: 1_000,
            maximum_emitters: 32,
            pool_bytes: 8 * 1024 * 1024,
            cpu_micros_per_frame: 2_000,
            gpu_micros_per_frame: 2_000,
            maximum_overdraw: 4.0,
        },
    };
    let sparks = VfxGraphContract {
        id: "vfx.sparks".to_owned(),
        nodes: vec![
            VfxNodeContract {
                id: "node.emitter".to_owned(),
                execution: VfxExecutionClass::Gpu,
                kind: VfxNodeKind::Emitter {
                    rate: 0.0,
                    burst: 16,
                },
            },
            VfxNodeContract {
                id: "node.size".to_owned(),
                execution: VfxExecutionClass::Gpu,
                kind: VfxNodeKind::SizeCurve { points: curve() },
            },
            VfxNodeContract {
                id: "node.smoke".to_owned(),
                execution: VfxExecutionClass::Cpu,
                kind: VfxNodeKind::SubEmitter {
                    graph: "vfx.smoke".to_owned(),
                },
            },
        ],
        edges: vec![
            VfxEdgeContract {
                from: "node.emitter".to_owned(),
                to: "node.size".to_owned(),
            },
            VfxEdgeContract {
                from: "node.size".to_owned(),
                to: "node.smoke".to_owned(),
            },
        ],
        lod: vec![
            VfxLodContract {
                distance: 0.0,
                spawn_scale: 1.0,
            },
            VfxLodContract {
                distance: 50.0,
                spawn_scale: 0.25,
            },
        ],
        budgets: smoke.budgets.clone(),
    };

    MediaContractSet {
        format: MEDIA_CONTRACT_FORMAT.to_owned(),
        capability_registry_hash: registry.hash.clone(),
        capability_ids: vec![
            "component.particle_emitter".to_owned(),
            "component.audio_source".to_owned(),
            "component.audio_listener".to_owned(),
        ],
        vfx: vec![smoke, sparks],
        audio: AudioContract {
            clips: vec![AudioClipContract {
                id: "audio.footstep".to_owned(),
                resource: RuntimeResourceHandle {
                    id: 1,
                    generation: 1,
                },
                duration_seconds: 0.4,
                channels: 1,
                sample_rate: 48_000,
                streaming: false,
            }],
            buses: vec![
                MixerBusContract {
                    id: "bus.master".to_owned(),
                    parent: None,
                    gain: 1.0,
                    effects: vec![AudioEffectContract {
                        kind: AudioEffectKind::Compressor,
                        enabled: true,
                        amount: 0.5,
                    }],
                },
                MixerBusContract {
                    id: "bus.sfx".to_owned(),
                    parent: Some("bus.master".to_owned()),
                    gain: 1.0,
                    effects: Vec::new(),
                },
            ],
            events: vec![AudioEventContract {
                id: "audio_event.footstep".to_owned(),
                priority: 100,
                spatial: SpatialAudioContract {
                    enabled: true,
                    attenuation: AttenuationContract {
                        minimum_distance: 1.0,
                        maximum_distance: 25.0,
                        rolloff: 1.0,
                    },
                    occlusion: 0.5,
                    reverb_send: 0.25,
                },
                actions: vec![AudioEventAction::Play {
                    clip: "audio.footstep".to_owned(),
                    bus: "bus.sfx".to_owned(),
                    looped: false,
                }],
            }],
            zones: vec![ReverbZoneContract {
                id: "zone.cave".to_owned(),
                center: [0.0, 0.0, 0.0],
                half_extents: [10.0, 5.0, 10.0],
                wet: 0.7,
                decay_seconds: 2.0,
            }],
            listener: Some(RuntimeEntityHandle {
                id: 2,
                generation: 1,
            }),
            budgets: AudioBudgetContract {
                maximum_voices: 64,
                streaming_bytes: 16 * 1024 * 1024,
                resident_bytes: 64 * 1024 * 1024,
                cpu_micros_per_frame: 2_000,
            },
        },
    }
}

#[test]
fn typed_vfx_and_audio_graphs_validate_with_registry_and_budgets() {
    let registry = registry();
    media_set(&registry)
        .validate(&registry)
        .expect("media contracts valid");
}

#[test]
fn vfx_cycles_bad_lod_and_dangling_audio_routes_fail_closed() {
    let registry = registry();

    let mut media = media_set(&registry);
    media.vfx[1].edges.push(VfxEdgeContract {
        from: "node.smoke".to_owned(),
        to: "node.emitter".to_owned(),
    });
    assert!(media.validate(&registry).is_err());

    let mut media = media_set(&registry);
    media.vfx[1].lod[1].distance = 0.0;
    assert!(media.validate(&registry).is_err());

    let mut media = media_set(&registry);
    media.audio.events[0].actions[0] = AudioEventAction::Play {
        clip: "audio.missing".to_owned(),
        bus: "bus.sfx".to_owned(),
        looped: false,
    };
    assert!(media.validate(&registry).is_err());

    let mut media = media_set(&registry);
    media.audio.buses[0].parent = Some("bus.sfx".to_owned());
    media.audio.buses[1].parent = Some("bus.master".to_owned());
    assert!(media.validate(&registry).is_err());
}

#[test]
fn audio_device_lifecycle_does_not_skip_open_or_recovery() {
    assert!(AudioDeviceState::Unavailable.allows(AudioDeviceState::Opening));
    assert!(AudioDeviceState::Opening.allows(AudioDeviceState::Ready));
    assert!(AudioDeviceState::Ready.allows(AudioDeviceState::Suspended));
    assert!(AudioDeviceState::Lost.allows(AudioDeviceState::Opening));
    assert!(!AudioDeviceState::Unavailable.allows(AudioDeviceState::Ready));
    assert!(!AudioDeviceState::Lost.allows(AudioDeviceState::Ready));
}
