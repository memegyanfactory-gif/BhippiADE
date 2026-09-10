//! The design base on disk matches the base compiled in (INV-091): every Markdown file
//! under `prompts/design/` is a bundled module, every bundled module is a file, and the
//! public surface behaves as `docs/18` describes.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use bhippi_core::{DesignKb, DesignQuery, DesignRequest, SearchQuery};
use bhippi_types::{DesignSurface, DESIGN_CONTEXT_TOKEN_BUDGET, DESIGN_INDEX_TOKEN_BUDGET};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

fn design_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("prompts")
        .join("design")
}

fn walk(dir: &Path, root: &Path, out: &mut BTreeSet<String>) {
    for entry in std::fs::read_dir(dir).expect("prompts/design is readable") {
        let entry = entry.expect("dir entry");
        let path = entry.path();
        if path.is_dir() {
            walk(&path, root, out);
            continue;
        }
        if path.extension().and_then(|e| e.to_str()) != Some("md") {
            continue;
        }
        if path.file_name().and_then(|n| n.to_str()) == Some("INDEX.md") {
            continue;
        }
        let rel = path
            .strip_prefix(root)
            .expect("under prompts/design")
            .with_extension("");
        let id = rel
            .components()
            .map(|c| c.as_os_str().to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join("/");
        out.insert(id);
    }
}

#[test]
fn every_file_on_disk_is_bundled_and_every_bundled_module_is_a_file() {
    let root = design_dir();
    let mut on_disk = BTreeSet::new();
    walk(&root, &root, &mut on_disk);
    let kb = DesignKb::bundled().expect("the bundled base parses");
    let bundled: BTreeSet<String> = kb.modules().iter().map(|m| m.id.clone()).collect();
    let missing: Vec<_> = on_disk.difference(&bundled).collect();
    let orphaned: Vec<_> = bundled.difference(&on_disk).collect();
    assert!(
        missing.is_empty(),
        "files under prompts/design/ not in MODULE_SOURCES: {missing:?}"
    );
    assert!(
        orphaned.is_empty(),
        "bundled modules with no file on disk: {orphaned:?}"
    );
    assert!(root.join("INDEX.md").is_file());
}

#[test]
fn the_index_is_a_map_and_the_pack_is_budgeted() {
    let kb = DesignKb::bundled().expect("parses");
    assert!(kb.index_tokens() <= DESIGN_INDEX_TOKEN_BUDGET);
    for surface in [
        DesignSurface::WebPage,
        DesignSurface::GameUi,
        DesignSurface::Scene3d,
        DesignSurface::Scene2d,
        DesignSurface::StudioChrome,
    ] {
        let pack = kb
            .select(&DesignRequest::new(surface).with_tags(["color", "type", "layout"]))
            .expect("selects");
        assert!(pack.tokens <= DESIGN_CONTEXT_TOKEN_BUDGET, "{surface:?}");
        assert!(!pack.sections.is_empty(), "{surface:?}");
    }
}

#[test]
fn the_model_can_reach_any_section_by_id_or_by_words() {
    let kb = DesignKb::bundled().expect("parses");
    for module in kb.modules() {
        for section in &module.sections {
            let answer = kb.answer(
                &DesignQuery::Section {
                    id: section.id.clone(),
                },
                None,
            );
            assert!(
                answer.text.contains(&section.title),
                "{} answers with its heading",
                section.id
            );
        }
        let hits = kb.search(&SearchQuery::new(module.title.clone()));
        assert!(
            hits.iter()
                .any(|h| h.id.starts_with(&format!("{}#", module.id))),
            "searching a module's own title reaches it: {} → {hits:?}",
            module.id
        );
    }
}
