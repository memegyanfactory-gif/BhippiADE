//! What one inspector is handed, and what it is expected to hand back.
//!
//! The context is the scope, already resolved. An inspector never asks "was I scoped to a
//! level or the whole project?" in order to decide *which files to read* — the answer is
//! already in `scenes`, `scripts` and `assets`. It asks only when the *meaning* of a check
//! changes with the scope, which is rare and always worth a comment: "nothing instances this
//! scene" is a finding about a project and a tautology about a level.

use bhippi_types::{INSPECT_MAX_SCENES, INSPECT_MAX_SCRIPTS};

use super::finding::Finding;
use super::measurement::PerformanceEvidence;
use super::report::{Coverage, InspectScope};
use super::snapshot::{AssetEntry, ProjectSnapshot, SceneEntry, ScriptEntry};

/// The scoped evidence one inspector may reason over.
pub struct InspectContext<'a> {
    pub snapshot: &'a ProjectSnapshot,
    pub scope: &'a InspectScope,
    pub scenes: Vec<&'a SceneEntry>,
    pub scripts: Vec<&'a ScriptEntry>,
    pub assets: Vec<&'a AssetEntry>,
    /// A measurement from a run that happened, or `None`. There is no third state.
    pub performance: Option<&'a PerformanceEvidence>,
}

impl<'a> InspectContext<'a> {
    /// Resolve a scope into the files it covers.
    #[must_use]
    pub fn new(
        snapshot: &'a ProjectSnapshot,
        scope: &'a InspectScope,
        performance: Option<&'a PerformanceEvidence>,
    ) -> Self {
        let (scenes, scripts, assets) = match scope {
            InspectScope::Project => (
                snapshot.scenes.iter().collect(),
                snapshot.scripts.iter().collect(),
                snapshot.assets.iter().collect(),
            ),
            InspectScope::Level { scene } => {
                let scenes = snapshot.scene_closure(scene);
                // The scripts a level's scenes attach, and nothing else: a level scope that
                // dragged in every script in the project would report the same code
                // findings from every level.
                let attached: Vec<&ScriptEntry> = snapshot
                    .scripts
                    .iter()
                    .filter(|script| {
                        scenes.iter().any(|entry| {
                            entry.parsed().is_some_and(|parsed| {
                                parsed
                                    .scripts()
                                    .iter()
                                    .any(|res| crate::godot::res_to_rel(res) == script.rel)
                            })
                        })
                    })
                    .collect();
                (scenes, attached, Vec::new())
            }
            InspectScope::Selection {
                scene,
                file,
                asset,
                node: _,
            } => {
                let scenes = scene
                    .as_deref()
                    .and_then(|rel| snapshot.scene(rel))
                    .into_iter()
                    .collect();
                let scripts = file
                    .as_deref()
                    .and_then(|rel| snapshot.script(rel))
                    .into_iter()
                    .collect();
                let assets = asset
                    .as_deref()
                    .and_then(|rel| {
                        snapshot
                            .assets
                            .iter()
                            .find(|entry| entry.rel == crate::godot::res_to_rel(rel))
                    })
                    .into_iter()
                    .collect();
                (scenes, scripts, assets)
            }
            InspectScope::Changes { files } => {
                let matches = |rel: &str| {
                    files
                        .iter()
                        .any(|changed| crate::godot::res_to_rel(changed) == rel)
                };
                (
                    snapshot
                        .scenes
                        .iter()
                        .filter(|entry| matches(&entry.rel))
                        .collect(),
                    snapshot
                        .scripts
                        .iter()
                        .filter(|entry| matches(&entry.rel))
                        .collect(),
                    snapshot
                        .assets
                        .iter()
                        .filter(|entry| matches(&entry.rel))
                        .collect(),
                )
            }
        };
        Self {
            snapshot,
            scope,
            scenes,
            scripts,
            assets,
            performance,
        }
    }

    /// True for a whole-project scan. The two checks that mean something different over a
    /// subset — "nothing reaches this scene", "no camera anywhere" — ask this.
    #[must_use]
    pub const fn is_project_scope(&self) -> bool {
        matches!(self.scope, InspectScope::Project)
    }

    #[must_use]
    pub fn scene_coverage(&self) -> Coverage {
        let items = u32::try_from(self.scenes.len()).unwrap_or(u32::MAX);
        if self.snapshot.scenes.len() >= INSPECT_MAX_SCENES {
            Coverage::Partial {
                items,
                reason: format!("stopped after {INSPECT_MAX_SCENES} scenes"),
            }
        } else {
            Coverage::Scanned { items }
        }
    }

    #[must_use]
    pub fn script_coverage(&self) -> Coverage {
        let items = u32::try_from(self.scripts.len()).unwrap_or(u32::MAX);
        let skipped = self
            .scripts
            .iter()
            .filter(|script| script.too_large)
            .count();
        if self.snapshot.scripts.len() >= INSPECT_MAX_SCRIPTS {
            Coverage::Partial {
                items,
                reason: format!("stopped after {INSPECT_MAX_SCRIPTS} scripts"),
            }
        } else if skipped > 0 {
            Coverage::Partial {
                items,
                reason: format!("{skipped} script(s) were too large to read"),
            }
        } else {
            Coverage::Scanned { items }
        }
    }

    #[must_use]
    pub fn asset_coverage(&self) -> Coverage {
        Coverage::Scanned {
            items: u32::try_from(self.assets.len()).unwrap_or(u32::MAX),
        }
    }
}

/// One inspector's answer: what it found, and what it actually saw.
///
/// The coverage is not decoration. It is what stops a project-health score from being
/// computed over an inspector that never ran (INV-097).
pub struct InspectorOutput {
    pub findings: Vec<Finding>,
    pub coverage: Coverage,
}

#[cfg(test)]
pub(crate) mod test_support {
    use super::*;
    use crate::godot::scene::GodotScene;
    use crate::inspect::snapshot::SceneEntry;

    /// A parsed scene entry from literal `.tscn` text.
    pub(crate) fn scene_entry(rel: &str, text: &str) -> SceneEntry {
        SceneEntry {
            rel: rel.to_owned(),
            res: format!("res://{rel}"),
            scene: Some(GodotScene::parse(text).expect("the fixture scene parses")),
            parse_error: None,
            vendored: false,
        }
    }

    /// A script entry from literal GDScript.
    pub(crate) fn script_entry(rel: &str, source: &str) -> ScriptEntry {
        ScriptEntry {
            rel: rel.to_owned(),
            res: format!("res://{rel}"),
            source: source.to_owned(),
            too_large: false,
            vendored: false,
        }
    }

    /// A whole-project context over a snapshot, with no performance measurement.
    pub(crate) fn context_from<'a>(snapshot: &'a ProjectSnapshot) -> InspectContext<'a> {
        InspectContext::new(snapshot, &InspectScope::Project, None)
    }
}

#[cfg(test)]
mod tests {
    use super::test_support::{context_from, scene_entry, script_entry};
    use super::*;

    fn snapshot() -> ProjectSnapshot {
        ProjectSnapshot {
            main_scene: Some("scenes/main.tscn".to_owned()),
            files: [
                "scenes/main.tscn".to_owned(),
                "scenes/door.tscn".to_owned(),
                "scripts/door.gd".to_owned(),
                "scripts/menu.gd".to_owned(),
            ]
            .into_iter()
            .collect(),
            scenes: vec![
                scene_entry(
                    "scenes/main.tscn",
                    "[gd_scene load_steps=2 format=3]\n\n[ext_resource type=\"PackedScene\" path=\"res://scenes/door.tscn\" id=\"1_d\"]\n\n[node name=\"Main\" type=\"Node3D\"]\n\n[node name=\"Door\" parent=\".\" instance=ExtResource(\"1_d\")]\n",
                ),
                scene_entry(
                    "scenes/door.tscn",
                    "[gd_scene load_steps=2 format=3]\n\n[ext_resource type=\"Script\" path=\"res://scripts/door.gd\" id=\"1_s\"]\n\n[node name=\"Door\" type=\"Node3D\"]\nscript = ExtResource(\"1_s\")\n",
                ),
            ],
            scripts: vec![
                script_entry("scripts/door.gd", "extends Node3D\n"),
                script_entry("scripts/menu.gd", "extends Control\n"),
            ],
            ..Default::default()
        }
    }

    #[test]
    fn a_level_scope_pulls_in_what_the_level_instances_and_stops_there() {
        let snapshot = snapshot();
        let scope = InspectScope::Level {
            scene: "scenes/main.tscn".to_owned(),
        };
        let context = InspectContext::new(&snapshot, &scope, None);
        let scenes: Vec<&str> = context
            .scenes
            .iter()
            .map(|entry| entry.rel.as_str())
            .collect();
        assert_eq!(scenes, vec!["scenes/door.tscn", "scenes/main.tscn"]);
        // Only the script the level's scenes actually attach.
        let scripts: Vec<&str> = context
            .scripts
            .iter()
            .map(|entry| entry.rel.as_str())
            .collect();
        assert_eq!(scripts, vec!["scripts/door.gd"]);
        assert!(!context.is_project_scope());
    }

    #[test]
    fn a_changes_scope_covers_exactly_the_files_the_caller_named() {
        let snapshot = snapshot();
        let scope = InspectScope::Changes {
            files: vec![
                "scripts/menu.gd".to_owned(),
                "res://scenes/door.tscn".to_owned(),
            ],
        };
        let context = InspectContext::new(&snapshot, &scope, None);
        assert_eq!(context.scripts.len(), 1);
        assert_eq!(context.scenes.len(), 1);
        assert_eq!(context.scenes[0].rel, "scenes/door.tscn");
    }

    #[test]
    fn a_project_scope_sees_everything_and_says_so() {
        let snapshot = snapshot();
        let context = context_from(&snapshot);
        assert!(context.is_project_scope());
        assert_eq!(context.scenes.len(), 2);
        assert_eq!(context.scene_coverage(), Coverage::Scanned { items: 2 });
    }

    #[test]
    fn a_script_too_large_to_read_makes_the_coverage_partial_rather_than_complete() {
        let mut snapshot = snapshot();
        snapshot.scripts[0].too_large = true;
        let context = context_from(&snapshot);
        assert!(matches!(
            context.script_coverage(),
            Coverage::Partial { .. }
        ));
    }
}
