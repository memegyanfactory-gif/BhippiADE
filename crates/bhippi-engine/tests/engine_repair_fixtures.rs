//! Typed repair fixtures used by the autonomous loop (ENG-188).

#![allow(clippy::expect_used, clippy::unwrap_used)]

use bhippi_engine::asset::{AssetIndex, AssetKind, AssetRecord, LicenseState};
use bhippi_engine::document::{Entity, SceneDocument};
use bhippi_engine::gates::check_assets;
use bhippi_types::{AssetId, EntityId};
use serde_json::json;

#[test]
fn dangling_asset_is_named_then_the_same_gate_passes_after_indexing() {
    let asset_id = AssetId::new();
    let reference = format!("asset:{asset_id}");
    let mut scene = SceneDocument::empty("warehouse");
    scene.entities.push(Entity {
        id: EntityId::new(),
        name: "Door".to_owned(),
        parent: None,
        tags: vec!["door".to_owned()],
        components: std::collections::BTreeMap::from([(
            "MeshRenderer".to_owned(),
            json!({"mesh": reference, "materials": [], "cast_shadows": true}),
        )]),
    });
    let scenes = [("assets/scenes/warehouse.bscn.json".to_owned(), scene)];
    let failure = check_assets(&AssetIndex::default(), &scenes, false);
    let dangling = failure
        .blockers()
        .into_iter()
        .find(|finding| finding.code == "dangling_asset")
        .expect("located dangling reference");
    assert!(dangling.message.contains(&asset_id.to_string()));

    let mut index = AssetIndex::default();
    index.assets.insert(
        asset_id,
        AssetRecord {
            id: asset_id,
            path_rel: "assets/models/door.glb".to_owned(),
            kind: AssetKind::Mesh,
            hash: "fixture".to_owned(),
            license: LicenseState::Known("CC0-1.0".to_owned()),
            size_bytes: 1,
            used_by_scenes: vec![],
        },
    );
    assert!(check_assets(&index, &scenes, false).passes());
}
