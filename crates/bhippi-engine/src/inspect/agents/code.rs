//! The Code Inspector: the project's own GDScript (ADR-0056 §2).
//!
//! GDScript is indentation-scoped, which is the one structural fact these checks lean on:
//! [`func_bodies`] recovers each function's lines without a parser, and that is enough to
//! answer "what happens every frame" — the question behind most of what makes a small game
//! stutter.
//!
//! Nothing here is a style opinion. A `print` left in a shipped build, a `preload` of a file
//! that is not there, a `while true` with no way out: each one is a fact about the file that
//! a person would want to know before they shipped it.

use bhippi_types::{
    InspectorId, Severity, INSPECT_CONFIDENCE_CERTAIN, INSPECT_CONFIDENCE_HEURISTIC,
};
use std::collections::BTreeSet;

use super::collect;
use crate::inspect::context::{InspectContext, InspectorOutput};
use crate::inspect::finding::{Finding, Location};

/// A scene attaches a script that is not on disk.
pub const CODE_MISSING_SCRIPT: &str = "BHP-INS-201";
/// A per-frame function does work that belongs in `_ready`.
pub const CODE_TICK_LOOKUP: &str = "BHP-INS-202";
/// A `TODO` / `FIXME` / `HACK` left in the source.
pub const CODE_TODO: &str = "BHP-INS-203";
/// A debug `print` left in the source.
pub const CODE_DEBUG_PRINT: &str = "BHP-INS-204";
/// A `preload` / `load` of a resource that is not on disk.
pub const CODE_DANGLING_LOAD: &str = "BHP-INS-205";
/// A script nothing attaches, preloads or autoloads.
pub const CODE_UNREFERENCED_SCRIPT: &str = "BHP-INS-206";
/// A `while true:` with no way out.
pub const CODE_UNBOUNDED_LOOP: &str = "BHP-INS-207";

/// The two functions Godot calls every frame.
const PER_FRAME: [&str; 2] = ["_process", "_physics_process"];

/// Calls that walk the scene tree or hit the disk. Cheap once in `_ready`, expensive at
/// sixty times a second.
const EXPENSIVE_IN_TICK: [&str; 6] = [
    "get_node(",
    "find_child(",
    "find_children(",
    "get_tree().get_nodes_in_group(",
    "load(",
    "instantiate(",
];

/// Comment markers worth surfacing. `XXX` is deliberately absent: it appears inside enough
/// hex literals and placeholder names to be noise.
const MARKERS: [&str; 3] = ["TODO", "FIXME", "HACK"];

#[must_use]
pub fn inspect(context: &InspectContext<'_>) -> InspectorOutput {
    let mut findings = Vec::new();

    // A scene pointing at a script that is not there. Godot loads the scene with no
    // behaviour attached, which looks exactly like a script that silently does nothing.
    for entry in &context.scenes {
        let Some(scene) = entry.parsed() else {
            continue;
        };
        for res in scene.scripts() {
            if context.snapshot.resolves(&res) {
                continue;
            }
            let rel = crate::godot::res_to_rel(&res);
            collect(
                &mut findings,
                Finding::draft(
                    InspectorId::Code,
                    CODE_MISSING_SCRIPT,
                    Severity::Critical,
                    INSPECT_CONFIDENCE_CERTAIN,
                    format!("{} attaches a script that is not on disk", entry.rel),
                    Location::scene(&entry.rel).with_symbol(rel.clone()),
                )
                .cause(format!(
                    "The scene references {res} and no file exists at that path."
                ))
                .impact(
                    "Every node that script was driving does nothing at run time, and Godot \
                     reports it as a load error rather than a crash — so the game starts and \
                     simply misbehaves.",
                )
                .recommend(format!(
                    "Restore {rel}, or re-attach the script that replaced it."
                ))
                .evidence(
                    format!("ext_resource type=\"Script\" path={res}"),
                    entry.rel.clone(),
                ),
            );
        }
    }

    for script in &context.scripts {
        if script.too_large {
            continue;
        }

        // Work that runs sixty times a second.
        for (name, start, body) in func_bodies(&script.source) {
            if !PER_FRAME.contains(&name.as_str()) {
                continue;
            }
            for (line_number, line) in &body {
                let trimmed = line.trim_start();
                if trimmed.starts_with('#') {
                    continue;
                }
                let Some(call) = EXPENSIVE_IN_TICK
                    .iter()
                    .find(|call| trimmed.contains(*call))
                else {
                    continue;
                };
                collect(
                    &mut findings,
                    Finding::draft(
                        InspectorId::Code,
                        CODE_TICK_LOOKUP,
                        Severity::High,
                        INSPECT_CONFIDENCE_HEURISTIC,
                        format!("`{call}` runs every frame in {name}"),
                        Location::line(&script.rel, *line_number),
                    )
                    .cause(format!(
                        "{name} is called once per frame, and line {line_number} does a tree \
                         walk or a resource load inside it."
                    ))
                    .impact(
                        "The cost is paid sixty times a second for a result that almost never \
                         changes. On a small scene it is a few frames; on a busy one it is the \
                         stutter.",
                    )
                    .recommend(format!(
                        "Resolve it once in _ready and keep the reference, or use an @onready \
                         variable, then use that inside {name}."
                    ))
                    .evidence(
                        trimmed.chars().take(120).collect::<String>(),
                        format!("{}:{line_number}", script.rel),
                    )
                    .evidence(
                        format!("{name} declared at line {start}"),
                        format!("{}:{start}", script.rel),
                    ),
                );
            }
        }

        for (line_number, line) in script.lines() {
            let trimmed = line.trim();

            // A marker somebody left for themselves.
            if let Some(marker) = MARKERS.iter().filter(|_| !script.vendored).find(|marker| {
                trimmed
                    .split_once('#')
                    .is_some_and(|(_, comment)| comment.contains(*marker))
            }) {
                collect(
                    &mut findings,
                    Finding::draft(
                        InspectorId::Code,
                        CODE_TODO,
                        Severity::Suggestion,
                        INSPECT_CONFIDENCE_CERTAIN,
                        format!("{marker} left in {}", script.rel),
                        Location::line(&script.rel, line_number),
                    )
                    .cause(format!(
                        "Line {line_number} carries a {marker} comment, which is a note to \
                         somebody that the code is not finished."
                    ))
                    .impact(
                        "Unfinished work that nobody is tracking. It is not a defect on its \
                         own — it is a defect that has already been noticed.",
                    )
                    .recommend("Do the work, or move the note somewhere it will be seen.")
                    .evidence(
                        trimmed.chars().take(120).collect::<String>(),
                        format!("{}:{line_number}", script.rel),
                    ),
                );
            }

            // A debug print left in.
            if !script.vendored
                && (trimmed.starts_with("print(") || trimmed.starts_with("print_debug("))
            {
                collect(
                    &mut findings,
                    Finding::draft(
                        InspectorId::Code,
                        CODE_DEBUG_PRINT,
                        Severity::Low,
                        INSPECT_CONFIDENCE_CERTAIN,
                        format!("Debug print in {}", script.rel),
                        Location::line(&script.rel, line_number),
                    )
                    .cause(
                        "The line prints to Godot's output, which stays in the exported \
                         build.",
                    )
                    .impact(
                        "Console noise in the shipped game, and a small per-call cost if it \
                         is in a hot path.",
                    )
                    .recommend(
                        "Delete it, or replace it with push_warning/push_error if the message \
                         is worth keeping.",
                    )
                    .evidence(
                        trimmed.chars().take(120).collect::<String>(),
                        format!("{}:{line_number}", script.rel),
                    ),
                );
            }

            // A load of something that is not there. Only an actual `load` argument
            // counts: a `res://` inside a format template or a tooltip is not a path
            // anybody will try to open.
            for res in loaded_res_paths(trimmed) {
                if context.snapshot.resolves(&res) {
                    continue;
                }
                collect(
                    &mut findings,
                    Finding::draft(
                        InspectorId::Code,
                        CODE_DANGLING_LOAD,
                        Severity::Critical,
                        INSPECT_CONFIDENCE_CERTAIN,
                        format!("{} loads a resource that is not on disk", script.rel),
                        Location::line(&script.rel, line_number).with_symbol(res.clone()),
                    )
                    .cause(format!(
                        "Line {line_number} loads {res}, and no file exists at that path."
                    ))
                    .impact(
                        "`preload` fails at parse time and stops the script loading at all; \
                         `load` returns null and the next line that uses it crashes the game.",
                    )
                    .recommend(format!(
                        "Restore {res}, or point the load at the resource that replaced it."
                    ))
                    .evidence(
                        trimmed.chars().take(120).collect::<String>(),
                        format!("{}:{line_number}", script.rel),
                    ),
                );
            }

            // A loop with no way out.
            if (trimmed == "while true:" || trimmed == "while 1:")
                && !loop_can_exit(&script.source, line_number)
            {
                collect(
                    &mut findings,
                    Finding::draft(
                        InspectorId::Code,
                        CODE_UNBOUNDED_LOOP,
                        Severity::High,
                        INSPECT_CONFIDENCE_HEURISTIC,
                        format!("`while true` with no exit in {}", script.rel),
                        Location::line(&script.rel, line_number),
                    )
                    .cause(
                        "The loop body contains no `break`, no `return` and no `await`, so \
                         nothing gives control back.",
                    )
                    .impact(
                        "Godot's main thread never reaches the next frame: the window stops \
                         responding and the game has to be killed.",
                    )
                    .recommend(
                        "Add the exit condition the loop is missing, or `await` a signal or \
                         a frame inside it.",
                    )
                    .evidence(trimmed.to_owned(), format!("{}:{line_number}", script.rel)),
                );
            }
        }
    }

    // A script nothing reaches. Only over the whole project: in a level scope the answer
    // would be "nothing in this level", which is not the same claim.
    if context.is_project_scope() {
        let attached = attached_scripts(context);
        let autoloads: BTreeSet<String> = context
            .snapshot
            .project_file
            .as_ref()
            .map(|file| {
                file.autoloads()
                    .iter()
                    .map(|autoload| crate::godot::res_to_rel(&autoload.path))
                    .collect()
            })
            .unwrap_or_default();
        for script in &context.scripts {
            if script.vendored
                || attached.contains(&script.rel)
                || autoloads.contains(&script.rel)
                || preloaded_by_another_script(context, &script.rel)
            {
                continue;
            }
            collect(
                &mut findings,
                Finding::draft(
                    InspectorId::Code,
                    CODE_UNREFERENCED_SCRIPT,
                    Severity::Suggestion,
                    INSPECT_CONFIDENCE_HEURISTIC,
                    format!("Nothing attaches or loads {}", script.rel),
                    Location::file(&script.rel),
                )
                .cause(
                    "No scene attaches it, no autoload registers it, and no other script \
                     preloads or extends it by path.",
                )
                .impact(
                    "It never runs. Changing it changes nothing, and reading it costs \
                     whoever comes next the time to work that out.",
                )
                .recommend(
                    "Attach it to the node it was written for, register it as an autoload, \
                     or delete it.",
                )
                .evidence(
                    "no ext_resource, autoload or preload names this path".to_owned(),
                    script.rel.clone(),
                ),
            );
        }
    }

    InspectorOutput {
        findings,
        coverage: context.script_coverage(),
    }
}

/// One function recovered from a source file: its name, the line it is declared on, and its
/// body as `(line number, text)`.
pub type FuncBody = (String, u32, Vec<(u32, String)>);

/// `(name, declaration line, body lines)` for each `func` in a GDScript source.
///
/// The body is every following line indented deeper than the `func` itself, which is what
/// GDScript's own scoping rule says. Blank lines inside a body do not end it.
#[must_use]
pub fn func_bodies(source: &str) -> Vec<FuncBody> {
    let mut out: Vec<FuncBody> = Vec::new();
    let lines: Vec<&str> = source.lines().collect();
    for (index, line) in lines.iter().enumerate() {
        let indent = indent_of(line);
        let trimmed = line.trim_start();
        let Some(rest) = trimmed.strip_prefix("func ") else {
            continue;
        };
        let Some(name) = rest.split('(').next().map(str::trim) else {
            continue;
        };
        let declared = u32::try_from(index).unwrap_or(u32::MAX).saturating_add(1);
        let mut body = Vec::new();
        for (offset, candidate) in lines.iter().enumerate().skip(index + 1) {
            if candidate.trim().is_empty() {
                continue;
            }
            if indent_of(candidate) <= indent {
                break;
            }
            body.push((
                u32::try_from(offset).unwrap_or(u32::MAX).saturating_add(1),
                (*candidate).to_owned(),
            ));
        }
        out.push((name.to_owned(), declared, body));
    }
    out
}

fn indent_of(line: &str) -> usize {
    line.chars()
        .take_while(|character| *character == ' ' || *character == '\t')
        .count()
}

/// The `res://…` paths this line actually *loads*.
///
/// Narrower than [`quoted_res_paths`] on purpose, and the difference is a bug this check
/// shipped with for exactly one test run: an addon that built a tooltip out of
/// `"res://%s" % name`, and another that called `ProjectSettings.globalize_path("res://")`,
/// were both reported as loading a file that was not there. A path is only a path when a
/// load is what is being done to it, and a template with a `%` or a `{` in it is not a path
/// at all.
#[must_use]
pub fn loaded_res_paths(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    for call in ["preload(", "load("] {
        let mut cursor = 0usize;
        while let Some(found) = line[cursor..].find(call) {
            let at = cursor + found;
            cursor = at + call.len();
            let Some(path) = super::gameplay::first_quoted_after(&line[at..], call) else {
                continue;
            };
            if !path.starts_with(crate::godot::RES_PREFIX)
                || path == crate::godot::RES_PREFIX
                || path.contains('%')
                || path.contains('{')
            {
                continue;
            }
            out.push(path);
        }
    }
    out.sort();
    out.dedup();
    out
}

/// Every `res://…` path inside double or single quotes on one line.
#[must_use]
pub fn quoted_res_paths(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    for quote in ['"', '\''] {
        let mut rest = line;
        while let Some(start) = rest.find(quote) {
            let after = &rest[start + 1..];
            let Some(end) = after.find(quote) else { break };
            let value = &after[..end];
            if value.starts_with(crate::godot::RES_PREFIX) {
                out.push(value.to_owned());
            }
            rest = &after[end + 1..];
        }
    }
    out.sort();
    out.dedup();
    out
}

/// True when the `while` block starting at `line_number` contains a way out.
fn loop_can_exit(source: &str, line_number: u32) -> bool {
    let lines: Vec<&str> = source.lines().collect();
    let index = usize::try_from(line_number).unwrap_or(0).saturating_sub(1);
    let Some(header) = lines.get(index) else {
        return true;
    };
    let indent = indent_of(header);
    lines
        .iter()
        .skip(index + 1)
        .take_while(|line| line.trim().is_empty() || indent_of(line) > indent)
        .any(|line| {
            let trimmed = line.trim_start();
            trimmed.starts_with("break")
                || trimmed.starts_with("return")
                || trimmed.contains("await ")
        })
}

/// Every script some scene attaches, project-relative.
fn attached_scripts(context: &InspectContext<'_>) -> BTreeSet<String> {
    context
        .snapshot
        .scenes
        .iter()
        .filter_map(|entry| entry.parsed())
        .flat_map(crate::godot::scene::GodotScene::scripts)
        .map(|res| crate::godot::res_to_rel(&res))
        .collect()
}

/// True when another script names this one's path.
fn preloaded_by_another_script(context: &InspectContext<'_>, rel: &str) -> bool {
    let res = format!("{}{rel}", crate::godot::RES_PREFIX);
    context.snapshot.scripts.iter().any(|other| {
        other.rel != rel && (other.source.contains(&res) || other.source.contains(rel))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inspect::context::test_support::{context_from, scene_entry, script_entry};
    use crate::inspect::snapshot::ProjectSnapshot;

    #[test]
    fn a_function_body_is_the_lines_indented_under_it() {
        let source =
            "extends Node\n\nfunc _ready():\n\tvar a = 1\n\n\tvar b = 2\n\nfunc other():\n\tpass\n";
        let bodies = func_bodies(source);
        assert_eq!(bodies.len(), 2);
        assert_eq!(bodies[0].0, "_ready");
        assert_eq!(bodies[0].1, 3);
        assert_eq!(bodies[0].2.len(), 2, "the blank line does not end the body");
        assert_eq!(bodies[1].0, "other");
    }

    #[test]
    fn a_tree_walk_in_process_is_reported_and_the_same_call_in_ready_is_not() {
        let snapshot = ProjectSnapshot {
            scripts: vec![script_entry(
                "scripts/player.gd",
                "extends Node3D\n\nfunc _ready():\n\tvar hud = get_node(\"HUD\")\n\nfunc _process(delta):\n\tvar hud = get_node(\"HUD\")\n",
            )],
            ..Default::default()
        };
        let output = inspect(&context_from(&snapshot));
        let ticks: Vec<_> = output
            .findings
            .iter()
            .filter(|finding| finding.code == CODE_TICK_LOOKUP)
            .collect();
        assert_eq!(ticks.len(), 1);
        assert_eq!(ticks[0].location.line, Some(7));
    }

    #[test]
    fn a_preload_of_a_missing_file_is_critical_and_one_that_resolves_is_silent() {
        let snapshot = ProjectSnapshot {
            files: ["scenes/door.tscn".to_owned()].into_iter().collect(),
            scripts: vec![script_entry(
                "scripts/spawn.gd",
                "extends Node\n\nconst DOOR = preload(\"res://scenes/door.tscn\")\nconst GONE = preload(\"res://scenes/gone.tscn\")\n",
            )],
            ..Default::default()
        };
        let output = inspect(&context_from(&snapshot));
        let dangling: Vec<_> = output
            .findings
            .iter()
            .filter(|finding| finding.code == CODE_DANGLING_LOAD)
            .collect();
        assert_eq!(dangling.len(), 1);
        assert_eq!(
            dangling[0].location.symbol.as_deref(),
            Some("res://scenes/gone.tscn")
        );
    }

    #[test]
    fn a_while_true_with_a_break_is_not_a_hang() {
        let snapshot = ProjectSnapshot {
            scripts: vec![script_entry(
                "scripts/loop.gd",
                "extends Node\n\nfunc a():\n\twhile true:\n\t\tbreak\n\nfunc b():\n\twhile true:\n\t\tvar x = 1\n",
            )],
            ..Default::default()
        };
        let output = inspect(&context_from(&snapshot));
        let hangs: Vec<_> = output
            .findings
            .iter()
            .filter(|finding| finding.code == CODE_UNBOUNDED_LOOP)
            .collect();
        assert_eq!(hangs.len(), 1);
        assert_eq!(hangs[0].location.line, Some(8));
    }

    #[test]
    fn a_script_a_scene_attaches_is_not_unreferenced() {
        let snapshot = ProjectSnapshot {
            files: [
                "scenes/door.tscn".to_owned(),
                "scripts/door.gd".to_owned(),
                "scripts/lonely.gd".to_owned(),
            ]
            .into_iter()
            .collect(),
            scenes: vec![scene_entry(
                "scenes/door.tscn",
                "[gd_scene load_steps=2 format=3]\n\n[ext_resource type=\"Script\" path=\"res://scripts/door.gd\" id=\"1_s\"]\n\n[node name=\"Door\" type=\"Node3D\"]\nscript = ExtResource(\"1_s\")\n",
            )],
            scripts: vec![
                script_entry("scripts/door.gd", "extends Node3D\n"),
                script_entry("scripts/lonely.gd", "extends Node\n"),
            ],
            ..Default::default()
        };
        let output = inspect(&context_from(&snapshot));
        let unreferenced: Vec<&str> = output
            .findings
            .iter()
            .filter(|finding| finding.code == CODE_UNREFERENCED_SCRIPT)
            .filter_map(|finding| finding.location.file.as_deref())
            .collect();
        assert_eq!(unreferenced, vec!["scripts/lonely.gd"]);
    }

    #[test]
    fn a_marker_in_a_comment_counts_and_the_same_word_in_a_string_does_not() {
        let snapshot = ProjectSnapshot {
            scripts: vec![script_entry(
                "scripts/notes.gd",
                "extends Node\n\nvar label = \"TODO list\"\n# TODO: wire the door\n",
            )],
            ..Default::default()
        };
        let output = inspect(&context_from(&snapshot));
        let markers: Vec<_> = output
            .findings
            .iter()
            .filter(|finding| finding.code == CODE_TODO)
            .collect();
        assert_eq!(markers.len(), 1);
        assert_eq!(markers[0].location.line, Some(4));
    }

    #[test]
    fn quoted_res_paths_finds_both_quote_styles_and_ignores_other_strings() {
        let found =
            quoted_res_paths("var a = load('res://a.png') + preload(\"res://b.tscn\") + \"plain\"");
        assert_eq!(
            found,
            vec!["res://a.png".to_owned(), "res://b.tscn".to_owned()]
        );
    }
}
