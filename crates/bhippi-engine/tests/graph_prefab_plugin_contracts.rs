#![allow(clippy::expect_used, clippy::unwrap_used)]

use bhippi_engine::behavior_graph::{
    BehaviorGraphDocument, BehaviorNodeContract, BehaviorNodeKind, GraphDebugContract,
    GraphEdgeContract, GraphPortContract, GraphValueType, BEHAVIOR_BYTECODE_FORMAT,
    BEHAVIOR_GRAPH_FORMAT,
};
use bhippi_engine::extension_contract::{
    CapabilityPackContract, ExposedParameterContract, ExposedParameterType, NestedPrefabContract,
    PluginExposure, PluginLifecycleState, PluginManifestContract, PluginRecoveryContract,
    PrefabEvolutionContract, PrefabMigrationContract, PrefabMigrationOperation,
    PrefabOverrideContract, PrefabVariantContract, PLUGIN_FORMAT, PREFAB_EVOLUTION_FORMAT,
};
use bhippi_engine::prefab::{PrefabDocument, PrefabNode};
use bhippi_engine::registry::{CapabilityRegistry, CostClass, ExtensionManifest, ENTRY_VERSION};
use serde_json::json;
use std::collections::BTreeMap;

fn registry() -> CapabilityRegistry {
    CapabilityRegistry::core().expect("registry")
}

fn flow_port(id: &str, required: bool) -> GraphPortContract {
    GraphPortContract {
        id: id.to_owned(),
        value_type: GraphValueType::Flow,
        required,
    }
}

fn behavior_graph(registry: &CapabilityRegistry) -> BehaviorGraphDocument {
    BehaviorGraphDocument {
        format: BEHAVIOR_GRAPH_FORMAT.to_owned(),
        id: "behavior.jump".to_owned(),
        capability_registry_hash: registry.hash.clone(),
        variables: BTreeMap::from([("can_jump".to_owned(), GraphValueType::Bool)]),
        nodes: vec![
            BehaviorNodeContract {
                id: "node.start".to_owned(),
                kind: BehaviorNodeKind::Event {
                    event: "game.start".to_owned(),
                },
                inputs: Vec::new(),
                outputs: vec![flow_port("next", false)],
                literals: BTreeMap::new(),
            },
            BehaviorNodeContract {
                id: "node.action".to_owned(),
                kind: BehaviorNodeKind::DispatchAction {
                    capability_id: "component.character_controller".to_owned(),
                    action_kind: "set_component_property".to_owned(),
                },
                inputs: vec![flow_port("in", true)],
                outputs: Vec::new(),
                literals: BTreeMap::new(),
            },
        ],
        edges: vec![GraphEdgeContract {
            from_node: "node.start".to_owned(),
            from_port: "next".to_owned(),
            to_node: "node.action".to_owned(),
            to_port: "in".to_owned(),
        }],
        debug: GraphDebugContract {
            breakpoints: vec!["node.action".to_owned()],
            watch_values: vec!["can_jump".to_owned()],
            trace_capacity: 128,
        },
    }
}

#[test]
fn typed_graph_compiles_to_deterministic_inert_actions_and_debug_indices() {
    let registry = registry();
    let graph = behavior_graph(&registry);
    let first = graph.compile(&registry).expect("compiles");
    let second = graph.compile(&registry).expect("compiles identically");
    assert_eq!(first, second);
    assert_eq!(first.format, BEHAVIOR_BYTECODE_FORMAT);
    assert_eq!(first.instructions.len(), 2);
    assert_eq!(first.breakpoints, vec![1]);
    assert_eq!(first.watched_variables, vec!["can_jump"]);
}

#[test]
fn graph_type_mismatch_cycle_and_unregistered_action_fail_closed() {
    let registry = registry();
    let mut graph = behavior_graph(&registry);
    graph.nodes[1].inputs[0].value_type = GraphValueType::Bool;
    assert!(graph.compile(&registry).is_err());

    let mut graph = behavior_graph(&registry);
    graph.nodes[1].outputs.push(flow_port("loop", false));
    graph.nodes[0].inputs.push(flow_port("again", false));
    graph.edges.push(GraphEdgeContract {
        from_node: "node.action".to_owned(),
        from_port: "loop".to_owned(),
        to_node: "node.start".to_owned(),
        to_port: "again".to_owned(),
    });
    assert!(graph.compile(&registry).is_err());

    let mut graph = behavior_graph(&registry);
    graph.nodes[1].kind = BehaviorNodeKind::DispatchAction {
        capability_id: "engine.telepathy".to_owned(),
        action_kind: "guess".to_owned(),
    };
    assert!(graph.compile(&registry).is_err());
}

fn prefab() -> PrefabDocument {
    let mut document = PrefabDocument::new("Crate");
    document.nodes = vec![PrefabNode {
        local_id: "node.root".to_owned(),
        name: "Crate".to_owned(),
        parent: None,
        tags: vec!["prop".to_owned()],
        components: BTreeMap::from([(
            "Transform".to_owned(),
            json!({"pos":[0.0,0.0,0.0],"rot":[0.0,0.0,0.0,1.0],"scale":[1.0,1.0,1.0]}),
        )]),
    }];
    document
}

fn evolution(prefab: &PrefabDocument) -> PrefabEvolutionContract {
    PrefabEvolutionContract {
        format: PREFAB_EVOLUTION_FORMAT.to_owned(),
        prefab_id: prefab.id.to_string(),
        version: "2.0.0".to_owned(),
        nested: vec![NestedPrefabContract {
            mount_node: "node.root".to_owned(),
            prefab_id: "prefab.handle".to_owned(),
            required_version: "1.0.0".to_owned(),
        }],
        exposed_parameters: vec![ExposedParameterContract {
            id: "parameter.scale".to_owned(),
            value_type: ExposedParameterType::Vec3,
            default: json!([1.0, 1.0, 1.0]),
            target_node: "node.root".to_owned(),
            component: "Transform".to_owned(),
            property_path: "scale".to_owned(),
        }],
        variants: vec![PrefabVariantContract {
            id: "variant.large".to_owned(),
            parent_variant: None,
            parameter_values: BTreeMap::from([(
                "parameter.scale".to_owned(),
                json!([2.0, 2.0, 2.0]),
            )]),
            overrides: vec![PrefabOverrideContract {
                target_node: "node.root".to_owned(),
                component: "Transform".to_owned(),
                property_path: "scale".to_owned(),
                expected_base_hash: "a".repeat(64),
                value: json!([2.0, 2.0, 2.0]),
            }],
            replicated: true,
            authority: Some("server".to_owned()),
        }],
        migrations: vec![PrefabMigrationContract {
            from_version: "1.0.0".to_owned(),
            to_version: "2.0.0".to_owned(),
            operations: vec![PrefabMigrationOperation::RenameParameter {
                from: "scale".to_owned(),
                to: "parameter.scale".to_owned(),
            }],
        }],
    }
}

#[test]
fn nested_prefab_variants_parameters_migrations_and_conflicts_are_deterministic() {
    let prefab = prefab();
    let contract = evolution(&prefab);
    let catalogue = BTreeMap::from([("prefab.handle".to_owned(), "1.0.0".to_owned())]);
    contract
        .validate(&prefab, &catalogue)
        .expect("evolution valid");
    let target = "node.root:Transform:scale".to_owned();
    let conflicts = contract.conflicts(&BTreeMap::from([(target.clone(), "b".repeat(64))]));
    assert_eq!(conflicts.len(), 1);
    assert_eq!(conflicts[0].target, target);
}

#[test]
fn prefab_nested_cycles_variant_cycles_and_mistyped_parameters_block() {
    let prefab = prefab();
    let catalogue = BTreeMap::from([("prefab.handle".to_owned(), "1.0.0".to_owned())]);

    let mut contract = evolution(&prefab);
    contract.nested[0].prefab_id = prefab.id.to_string();
    assert!(contract.validate(&prefab, &catalogue).is_err());

    let mut contract = evolution(&prefab);
    contract.variants[0].parent_variant = Some("variant.large".to_owned());
    assert!(contract.validate(&prefab, &catalogue).is_err());

    let mut contract = evolution(&prefab);
    contract.variants[0]
        .parameter_values
        .insert("parameter.scale".to_owned(), json!("huge"));
    assert!(contract.validate(&prefab, &catalogue).is_err());
}

fn plugin(registry: &CapabilityRegistry) -> PluginManifestContract {
    let mut pack_entry = registry.entries[0].clone();
    pack_entry.id = "plugin.example.feature".to_owned();
    pack_entry.name = "Example feature".to_owned();
    pack_entry.owner = "plugin.example".to_owned();
    pack_entry.licence = "MIT".to_owned();
    pack_entry.provenance = "fixture plugin manifest".to_owned();
    PluginManifestContract {
        format: PLUGIN_FORMAT.to_owned(),
        id: "plugin.example".to_owned(),
        version: ENTRY_VERSION.to_owned(),
        extension: ExtensionManifest {
            id: "plugin.example".to_owned(),
            version: ENTRY_VERSION.to_owned(),
            dependencies: vec!["component.transform".to_owned()],
            permissions: vec!["create_content".to_owned()],
            config: Vec::new(),
            runtime_exposed: true,
            editor_exposed: false,
            ai_exposed: true,
            cost: CostClass::Low,
            platforms: vec!["windows".to_owned()],
            licence: "MIT".to_owned(),
            provenance: "fixture plugin manifest".to_owned(),
        },
        dependencies: Vec::new(),
        exposures: vec![PluginExposure::CapabilityPack],
        pack: Some(CapabilityPackContract {
            entries: vec![pack_entry],
        }),
    }
}

#[test]
fn plugin_capability_pack_is_additive_registry_truth_with_recovery_lifecycle() {
    let registry = registry();
    let expanded = plugin(&registry)
        .validate(&registry, &BTreeMap::new())
        .expect("plugin valid")
        .expect("pack creates expanded registry");
    assert!(expanded.describe("plugin.example.feature").is_some());
    assert!(PluginLifecycleState::Staged.allows(PluginLifecycleState::Validated));
    assert!(PluginLifecycleState::Disabled.allows(PluginLifecycleState::Removing));
    assert!(!PluginLifecycleState::Staged.allows(PluginLifecycleState::Active));
    PluginRecoveryContract {
        staged_manifest_hash: "a".repeat(64),
        previous_manifest_hash: Some("b".repeat(64)),
        rollback_required_on_fault: true,
        preserve_diagnostic: true,
    }
    .validate()
    .expect("recoverable plan");
}

#[test]
fn hostile_plugin_unknown_authority_core_collision_and_missing_provenance_block() {
    let registry = registry();

    let mut manifest = plugin(&registry);
    manifest.extension.permissions = vec!["filesystem_root".to_owned()];
    assert!(manifest.validate(&registry, &BTreeMap::new()).is_err());

    let mut manifest = plugin(&registry);
    manifest.pack.as_mut().expect("pack").entries[0].id = registry.entries[0].id.clone();
    assert!(manifest.validate(&registry, &BTreeMap::new()).is_err());

    let mut manifest = plugin(&registry);
    manifest.pack.as_mut().expect("pack").entries[0]
        .provenance
        .clear();
    assert!(manifest.validate(&registry, &BTreeMap::new()).is_err());

    assert!(PluginRecoveryContract {
        staged_manifest_hash: "short".to_owned(),
        previous_manifest_hash: None,
        rollback_required_on_fault: false,
        preserve_diagnostic: false,
    }
    .validate()
    .is_err());
}
