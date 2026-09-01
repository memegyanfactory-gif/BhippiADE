#![allow(clippy::expect_used)]

use bhippi_engine::world_contract::{
    BiomeDocument, ProceduralGraphDocument, ProceduralNodeKind, ProceduralValueType,
    StreamingBudget, StreamingCellCoord, TerrainChunkCoord, TerrainDocument,
    WorldPartitionDocument,
};
use std::collections::BTreeSet;

const TERRAIN: &str =
    include_str!("../../../tests/fixtures/engine/world_contract/valid_terrain.json");
const BIOME: &str = include_str!("../../../tests/fixtures/engine/world_contract/valid_biome.json");
const GRAPH: &str =
    include_str!("../../../tests/fixtures/engine/world_contract/valid_procedural_graph.json");
const PARTITION: &str =
    include_str!("../../../tests/fixtures/engine/world_contract/valid_partition.json");

#[test]
fn versioned_world_documents_round_trip_from_committed_fixtures() {
    let terrain = TerrainDocument::parse(TERRAIN).expect("terrain fixture");
    assert_eq!(
        TerrainDocument::parse(&terrain.dump().expect("terrain dump")).expect("terrain reparse"),
        terrain
    );
    let biome = BiomeDocument::parse(BIOME).expect("biome fixture");
    assert_eq!(
        BiomeDocument::parse(&biome.dump().expect("biome dump")).expect("biome reparse"),
        biome
    );
    let graph = ProceduralGraphDocument::parse(GRAPH).expect("graph fixture");
    assert_eq!(
        ProceduralGraphDocument::parse(&graph.dump().expect("graph dump")).expect("graph reparse"),
        graph
    );
    let partition = WorldPartitionDocument::parse(PARTITION).expect("partition fixture");
    assert_eq!(
        WorldPartitionDocument::parse(&partition.dump().expect("partition dump"))
            .expect("partition reparse"),
        partition
    );
}

#[test]
fn terrain_bake_plan_is_seeded_deterministic_and_override_bound() {
    let terrain = TerrainDocument::parse(TERRAIN).expect("terrain fixture");
    let first = terrain.bake_plan().expect("bake plan");
    let second = terrain.bake_plan().expect("stable bake plan");
    assert_eq!(first, second);
    assert_eq!(first.chunks.len(), 4);
    assert!(first.chunks.iter().all(|chunk| {
        chunk.output_path.starts_with("assets/generated/terrain/")
            && chunk.collision_requested
            && chunk.lod_levels == 5
    }));

    let mut edited = terrain;
    edited.manual_overrides[0].height_m += 1.0;
    let edited_plan = edited.bake_plan().expect("edited plan");
    assert_ne!(first.source_hash, edited_plan.source_hash);
    assert_ne!(
        first.manual_overrides_hash,
        edited_plan.manual_overrides_hash
    );
}

#[test]
fn biome_scatter_candidates_are_deterministic_bounded_and_not_surface_claims() {
    let terrain = TerrainDocument::parse(TERRAIN).expect("terrain fixture");
    let biome = BiomeDocument::parse(BIOME).expect("biome fixture");
    let cell = TerrainChunkCoord { x: 1, z: 0 };
    let first = biome
        .plan_cell_scatter(&terrain, cell)
        .expect("scatter candidates");
    let second = biome
        .plan_cell_scatter(&terrain, cell)
        .expect("stable candidates");
    assert_eq!(first, second);
    assert_eq!(first.len(), 2);
    assert!(first.iter().all(|rule| {
        rule.requires_surface_projection
            && rule.candidates.len() <= 64
            && rule.candidates.iter().all(|point| point[1] == 0.0)
    }));
}

#[test]
fn typed_procedural_graph_compiles_in_dependency_order_and_fails_wrong_types() {
    let graph = ProceduralGraphDocument::parse(GRAPH).expect("graph fixture");
    let first = graph.compile().expect("program");
    let second = graph.compile().expect("stable program");
    assert_eq!(first, second);
    assert_eq!(first.output_type, ProceduralValueType::Points);
    assert_eq!(
        first.operations.last().map(|node| node.id.as_str()),
        Some("projected")
    );
    let bake = graph
        .bake_plan(&["assets/generated/village/lots.json".to_owned()])
        .expect("provenance-bound bake plan");
    assert_eq!(bake.source_hash, first.source_hash);
    assert_eq!(bake.artifacts.len(), 1);
    assert!(graph
        .bake_plan(&["assets/../escape.json".to_owned()])
        .expect_err("unsafe output")
        .hint()
        .is_some());

    let mut invalid = graph;
    let output = invalid
        .nodes
        .iter_mut()
        .find(|node| node.id == "projected")
        .expect("output node");
    if let ProceduralNodeKind::ProjectToField { points, .. } = &mut output.node {
        *points = "field".to_owned();
    }
    let error = invalid
        .compile()
        .expect_err("field cannot feed points socket");
    assert!(error.to_string().contains("needs Points"));
}

#[test]
fn streaming_plan_is_hash_bound_prioritised_bounded_and_cancellable() {
    let partition = WorldPartitionDocument::parse(PARTITION).expect("partition fixture");
    let resident = BTreeSet::from([StreamingCellCoord { x: 0, z: 0 }]);
    let desired = BTreeSet::from([
        StreamingCellCoord { x: 0, z: 0 },
        StreamingCellCoord { x: 1, z: 0 },
    ]);
    let budget = StreamingBudget {
        max_concurrent_loads: 2,
        max_resident_cells: 4,
        max_resident_memory_mb: 512,
        max_queue: 8,
        request_timeout_ms: 5_000,
    };
    let first = partition
        .plan_streaming(
            StreamingCellCoord { x: 0, z: 0 },
            &resident,
            &desired,
            budget,
        )
        .expect("streaming plan");
    let second = partition
        .plan_streaming(
            StreamingCellCoord { x: 0, z: 0 },
            &resident,
            &desired,
            budget,
        )
        .expect("stable streaming plan");
    assert_eq!(first, second);
    assert_eq!(first.loads.len(), 1);
    assert!(first.unloads.is_empty());
    assert!(first.cancellation_supported);
    assert!(first.loads[0]
        .cancellation_token
        .ends_with(&first.loads[0].request_id));
    assert_eq!(first.loads[0].partition_hash, first.partition_hash);
    partition
        .validate_request(&first.loads[0])
        .expect("current request");
    let mut stale = first.loads[0].clone();
    stale.partition_hash = "stale".to_owned();
    assert!(partition
        .validate_request(&stale)
        .expect_err("stale request")
        .to_string()
        .contains("stale"));
}

#[test]
fn streaming_contract_rejects_cycles_unknown_cells_and_memory_overcommit() {
    let mut partition = WorldPartitionDocument::parse(PARTITION).expect("partition fixture");
    partition.cells[0].dependencies = vec![StreamingCellCoord { x: 1, z: 0 }];
    let error = partition.validate().expect_err("dependency cycle");
    assert!(error.to_string().contains("cycle"));

    let partition = WorldPartitionDocument::parse(PARTITION).expect("partition fixture");
    let unknown = BTreeSet::from([StreamingCellCoord { x: 99, z: 99 }]);
    let error = partition
        .plan_streaming(
            StreamingCellCoord { x: 0, z: 0 },
            &BTreeSet::new(),
            &unknown,
            StreamingBudget {
                max_concurrent_loads: 1,
                max_resident_cells: 4,
                max_resident_memory_mb: 512,
                max_queue: 8,
                request_timeout_ms: 1_000,
            },
        )
        .expect_err("unknown desired cell");
    assert!(error.to_string().contains("unknown cell"));

    let all = BTreeSet::from([
        StreamingCellCoord { x: 0, z: 0 },
        StreamingCellCoord { x: 1, z: 0 },
    ]);
    let error = partition
        .plan_streaming(
            StreamingCellCoord { x: 0, z: 0 },
            &BTreeSet::new(),
            &all,
            StreamingBudget {
                max_concurrent_loads: 1,
                max_resident_cells: 4,
                max_resident_memory_mb: 100,
                max_queue: 8,
                request_timeout_ms: 1_000,
            },
        )
        .expect_err("memory overcommit");
    assert!(error.to_string().contains("MiB"));
}
