//! Regenerates the committed Phase-8 engine fixtures deterministically where identity matters.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use bhippi_engine::document::{Entity, SceneDocument};
use bhippi_engine::scaffold;
use bhippi_types::{EntityId, SceneId};
use serde_json::json;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::str::FromStr;

fn tree_hashes(root: &std::path::Path) -> BTreeMap<String, String> {
    fn visit(base: &std::path::Path, at: &std::path::Path, out: &mut BTreeMap<String, String>) {
        let mut entries = std::fs::read_dir(at)
            .expect("fixture directory")
            .filter_map(Result::ok)
            .collect::<Vec<_>>();
        entries.sort_by_key(std::fs::DirEntry::file_name);
        for entry in entries {
            let path = entry.path();
            if path.is_dir() {
                visit(base, &path, out);
            } else if entry.file_name() != "fixture-hashes.json" {
                let rel = path
                    .strip_prefix(base)
                    .expect("fixture-relative")
                    .to_string_lossy()
                    .replace('\\', "/");
                let bytes = std::fs::read(&path).expect("fixture bytes");
                out.insert(rel, blake3::hash(&bytes).to_hex().to_string());
            }
        }
    }
    let mut hashes = BTreeMap::new();
    visit(root, root, &mut hashes);
    hashes
}

fn fixed_id(index: usize) -> EntityId {
    const ALPHABET: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";
    let mut value = index;
    let mut suffix = [b'0'; 4];
    for slot in suffix.iter_mut().rev() {
        *slot = ALPHABET[value % 32];
        value /= 32;
    }
    let suffix = std::str::from_utf8(&suffix).expect("base32");
    EntityId::from_str(&format!("0000000000000000000000{suffix}")).expect("fixed ULID")
}

fn perf_scene(count: usize) -> SceneDocument {
    let mut scene = SceneDocument::empty("perf_1000");
    scene.id = SceneId::from_str("00000000000000000000000001").expect("scene id");
    let root = fixed_id(1);
    scene.entities.push(Entity {
        id: root,
        name: "PerfRoot".to_owned(),
        parent: None,
        tags: vec!["fixture".to_owned()],
        components: BTreeMap::from([(
            "Transform".to_owned(),
            json!({"pos":[0.0,0.0,0.0],"rot":[0.0,0.0,0.0,1.0],"scale":[1.0,1.0,1.0]}),
        )]),
    });
    for index in 0..count {
        scene.entities.push(Entity {
            id: fixed_id(index + 2),
            name: format!("Crate_{index:04}"),
            parent: Some(root),
            tags: vec!["prop".to_owned()],
            components: BTreeMap::from([
                ("Transform".to_owned(), json!({"pos":[(index % 40) as f32,0.5,(index / 40) as f32],"rot":[0.0,0.0,0.0,1.0],"scale":[1.0,1.0,1.0]})),
                ("MeshRenderer".to_owned(), json!({"mesh":"builtin:cube","materials":[],"cast_shadows":true})),
            ]),
        });
    }
    scene.validate().expect("perf fixture validates");
    scene
}

fn main() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests")
        .join("fixtures")
        .join("engine");
    std::fs::create_dir_all(&root).expect("fixture root");
    std::fs::write(
        root.join("perf_1000.bscn.json"),
        perf_scene(1_000).dump().expect("perf dump"),
    )
    .expect("perf fixture");

    let warehouse = root.join("warehouse_game");
    scaffold::write_project(&warehouse, "Warehouse Golden", true).expect("warehouse scaffold");

    let unlicensed = root.join("unlicensed_release");
    scaffold::write_project(&unlicensed, "Unlicensed Release", true).expect("license scaffold");
    std::fs::create_dir_all(unlicensed.join("assets/textures")).expect("texture folder");
    std::fs::write(
        unlicensed.join("assets/textures/unlicensed.png"),
        b"fixture image bytes",
    )
    .expect("unlicensed fixture");

    let crash = root.join("crash_recovery");
    scaffold::write_project(&crash, "Crash Recovery", true).expect("crash scaffold");

    let hashes = tree_hashes(&root);
    std::fs::write(
        root.join("fixture-hashes.json"),
        serde_json::to_string_pretty(&hashes).expect("hash manifest"),
    )
    .expect("hash manifest file");
}
