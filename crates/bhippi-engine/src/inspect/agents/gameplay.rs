//! The Gameplay Inspector: can this actually be played through? (ADR-0056 §2)
//!
//! Every other inspector asks whether a thing is well made. This one asks whether the things
//! are *connected* — whether the logic that exists can be reached from the logic that runs.
//! The failures it looks for are the ones that never produce an error message: a trigger
//! wired to nothing, a group queried that nobody joins, an input action the game reads and
//! the project never defined. Godot prints nothing for any of them. The player just finds a
//! door that will not open.
//!
//! The evidence is always a pair — *this exists* and *nothing reaches it* — because either
//! half alone proves nothing.

use bhippi_types::{
    FixRisk, InspectorId, Severity, INSPECT_CONFIDENCE_CERTAIN, INSPECT_CONFIDENCE_HEURISTIC,
};
use std::collections::BTreeSet;

use super::code::func_bodies;
use super::collect;
use crate::godot::action::GodotAction;
use crate::inspect::context::{InspectContext, InspectorOutput};
use crate::inspect::finding::{Finding, Location};
use crate::inspect::fix::ProposedFix;
use crate::inspect::nodes::{nodes, NodeRef};
use crate::inspect::snapshot::SceneEntry;

/// Nothing in the playable set can be possessed as a player.
pub const CODE_NO_PLAYER: &str = "BHP-INS-301";
/// A trigger volume whose overlap signal reaches nothing.
pub const CODE_TRIGGER_UNWIRED: &str = "BHP-INS-302";
/// A group the scripts query and nothing joins.
pub const CODE_EMPTY_GROUP: &str = "BHP-INS-303";
/// An input action the scripts read and `project.godot` does not define.
pub const CODE_UNDEFINED_INPUT: &str = "BHP-INS-304";
/// A signal a script emits and nothing listens to.
pub const CODE_UNHEARD_SIGNAL: &str = "BHP-INS-305";

/// The overlap signals an `Area` has, and which a trigger is authored for.
const TRIGGER_SIGNALS: [&str; 2] = ["body_entered", "area_entered"];

/// Godot defines these itself; a project that never touched Input Map still has them.
const BUILT_IN_INPUT_PREFIX: &str = "ui_";

/// The calls that read an input action by name.
const INPUT_CALLS: [&str; 5] = [
    "is_action_pressed(",
    "is_action_just_pressed(",
    "is_action_just_released(",
    "get_action_strength(",
    "is_action(",
];

/// The calls that ask the tree for a group by name.
const GROUP_QUERIES: [&str; 3] = [
    "get_nodes_in_group(",
    "get_first_node_in_group(",
    "is_in_group(",
];

#[must_use]
pub fn inspect(context: &InspectContext<'_>) -> InspectorOutput {
    let mut findings = Vec::new();

    player_start(context, &mut findings);
    triggers(context, &mut findings);
    groups(context, &mut findings);
    input_actions(context, &mut findings);
    signals(context, &mut findings);

    InspectorOutput {
        findings,
        coverage: context.scene_coverage(),
    }
}

/// Something the player is.
///
/// Only asked of a project that is *trying to be a game*. A freshly scaffolded Empty3D
/// project has no player, and that is not a defect — it is an empty project, and telling
/// somebody their brand-new game is Critically broken on the first scan is how a findings
/// list loses its reader. "Something reads input" is the cheapest honest signal that
/// gameplay has started.
fn player_start(context: &InspectContext<'_>, findings: &mut Vec<Finding>) {
    if context.scenes.is_empty() || !reads_input(context) {
        return;
    }
    let found = context.scenes.iter().any(|entry| {
        entry.parsed().is_some_and(|scene| {
            nodes(scene).iter().any(|node| {
                node.type_ends_with(&["CharacterBody3D", "CharacterBody2D"])
                    || node.name.eq_ignore_ascii_case("PlayerStart")
                    || node.name.eq_ignore_ascii_case("Player")
                    || node.raw.groups.iter().any(|group| group == "player")
            })
        })
    });
    if found {
        return;
    }
    let where_ = context
        .snapshot
        .main_scene
        .clone()
        .or_else(|| context.scenes.first().map(|entry| entry.rel.clone()))
        .unwrap_or_else(|| "project.godot".to_owned());
    collect(
        findings,
        Finding::draft(
            InspectorId::Gameplay,
            CODE_NO_PLAYER,
            Severity::Critical,
            INSPECT_CONFIDENCE_HEURISTIC,
            "Nothing in the playable scenes is a player",
            Location::scene(&where_),
        )
        .cause(format!(
            "Across {} scene(s) there is no CharacterBody2D/3D, no node named Player or \
             PlayerStart, and no node in the `player` group.",
            context.scenes.len()
        ))
        .impact("There is nothing for the person holding the controller to be.")
        .recommend(
            "Add the player character to the main scene, or put the existing one in the \
             `player` group so the rest of the game can find it.",
        )
        .evidence(
            format!("{} scene(s) read", context.scenes.len()),
            where_.clone(),
        ),
    );
}

/// True when some script of the project's own reads player input.
fn reads_input(context: &InspectContext<'_>) -> bool {
    context.snapshot.scripts.iter().any(|script| {
        !script.vendored
            && (script.source.contains("Input.")
                || script.source.contains("func _input(")
                || script.source.contains("func _unhandled_input("))
    })
}

/// A trigger volume nothing listens to. The single most common "why does nothing happen".
fn triggers(context: &InspectContext<'_>, findings: &mut Vec<Finding>) {
    for entry in &context.scenes {
        let Some(scene) = entry.parsed() else {
            continue;
        };
        for node in nodes(scene) {
            if !node.type_ends_with(&["Area2D", "Area3D"]) {
                continue;
            }
            let owner = scripted_owner(context, entry, &node);

            // Which of the two overlap signals this node is actually authored for. A
            // handler settles it; failing that, a node *named* like a trigger is reported
            // once, for `body_entered`, which is what almost every trigger wants. Reporting
            // both signals on every Area in the project would be noise, and noise is how a
            // findings list stops being read.
            let handled: Vec<(&str, Option<String>)> = TRIGGER_SIGNALS
                .iter()
                .filter_map(|signal| {
                    owner
                        .as_ref()
                        .and_then(|(_, source)| handler_for(source, signal))
                        .map(|handler| (*signal, Some(handler)))
                })
                .collect();
            let named_like_a_trigger = node.name.to_ascii_lowercase().contains("trigger")
                || node.name.to_ascii_lowercase().contains("area");
            let candidates: Vec<(&str, Option<String>)> = if handled.is_empty() {
                if named_like_a_trigger {
                    vec![("body_entered", None)]
                } else {
                    Vec::new()
                }
            } else {
                handled
            };

            for (signal, handler) in candidates {
                if scene
                    .document
                    .connections
                    .iter()
                    .any(|connection| connection.from == node.path && connection.signal == signal)
                {
                    continue;
                }
                // A script may connect it at run time; that is wiring too.
                if owner
                    .as_ref()
                    .is_some_and(|(_, source)| script_connects(source, signal))
                {
                    continue;
                }

                let scene_rel = entry.rel.clone();
                let mut draft = Finding::draft(
                    InspectorId::Gameplay,
                    CODE_TRIGGER_UNWIRED,
                    if handler.is_some() {
                        Severity::High
                    } else {
                        Severity::Medium
                    },
                    if handler.is_some() {
                        97
                    } else {
                        INSPECT_CONFIDENCE_HEURISTIC
                    },
                    format!("`{}` never delivers {signal}", node.path),
                    Location::node(&scene_rel, node.path).with_symbol(signal),
                )
                .cause(match &handler {
                    Some(name) => format!(
                        "The overlap handler `{name}` exists, the scene has no connection \
                         from `{}` for {signal}, and no script connects it at run time.",
                        node.path
                    ),
                    None => format!(
                        "`{}` is authored as a trigger volume and nothing — no scene \
                         connection, no `connect` call — listens to {signal}.",
                        node.path
                    ),
                })
                .impact(
                    "The volume fires the signal every time something walks into it and \
                     nothing runs. Whatever the trigger was for never happens, and Godot \
                     reports nothing.",
                )
                .recommend(match &handler {
                    Some(name) => format!("Connect {signal} on `{}` to `{name}`.", node.path),
                    None => format!(
                        "Connect {signal} on `{}` to the function that should run, or \
                         remove the volume.",
                        node.path
                    ),
                })
                .evidence(
                    format!(
                        "no [connection] with from=\"{}\" signal=\"{signal}\"",
                        node.path
                    ),
                    format!("{scene_rel}#{}", node.path),
                );

                if let (Some(method), Some((script_rel, _))) = (&handler, &owner) {
                    draft =
                        draft.evidence(format!("func {method} is declared"), script_rel.clone());
                    let target = script_owner_path(scene, &node).unwrap_or_else(|| ".".to_owned());
                    let action = GodotAction::ConnectSignal {
                        scene: scene_rel.clone(),
                        from: node.path.to_owned(),
                        signal: signal.to_owned(),
                        to: target,
                        method: method.clone(),
                    };
                    match ProposedFix::new(
                        CODE_TRIGGER_UNWIRED,
                        format!("Connect {signal} on `{}` to `{method}`", node.path),
                        FixRisk::Low,
                        vec![action],
                    ) {
                        Ok(fix) => draft = draft.fix(fix),
                        Err(error) => {
                            tracing::error!(%error, "the trigger fix could not be described");
                        }
                    }
                }
                collect(findings, draft);
            }
        }
    }
}

/// A group the game asks for and nothing is in.
fn groups(context: &InspectContext<'_>, findings: &mut Vec<Finding>) {
    if !context.is_project_scope() {
        return;
    }
    let mut joined: BTreeSet<String> = BTreeSet::new();
    for entry in &context.snapshot.scenes {
        let Some(scene) = entry.parsed() else {
            continue;
        };
        for node in nodes(scene) {
            joined.extend(node.raw.groups.iter().cloned());
        }
    }
    for script in &context.snapshot.scripts {
        for (_, line) in script.lines() {
            if let Some(group) = first_quoted_after(line, "add_to_group(") {
                joined.insert(group);
            }
        }
    }

    for script in &context.scripts {
        for (line_number, line) in script.lines() {
            for call in GROUP_QUERIES {
                let Some(group) = first_quoted_after(line, call) else {
                    continue;
                };
                if joined.contains(&group) {
                    continue;
                }
                collect(
                    findings,
                    Finding::draft(
                        InspectorId::Gameplay,
                        CODE_EMPTY_GROUP,
                        Severity::High,
                        INSPECT_CONFIDENCE_HEURISTIC,
                        format!("Nothing is ever in the `{group}` group"),
                        Location::line(&script.rel, line_number).with_symbol(group.clone()),
                    )
                    .cause(format!(
                        "Line {line_number} asks the tree for `{group}`. No node in any \
                         scene declares that group and no script adds anything to it."
                    ))
                    .impact(
                        "The query returns an empty list. Whatever depends on it — a pickup \
                         count, an objective, an enemy sweep — silently never happens, which \
                         is exactly how a mission becomes impossible to complete.",
                    )
                    .recommend(format!(
                        "Put the nodes that belong in `{group}` into the group, in the scene \
                         or with add_to_group, or fix the name if it is a typo."
                    ))
                    .evidence(
                        line.trim().chars().take(120).collect::<String>(),
                        format!("{}:{line_number}", script.rel),
                    )
                    .evidence(
                        format!("no node declares group \"{group}\""),
                        "every scene in the project".to_owned(),
                    ),
                );
            }
        }
    }
}

/// An input action the game reads and the project never defined.
fn input_actions(context: &InspectContext<'_>, findings: &mut Vec<Finding>) {
    let Some(project) = context.snapshot.project_file.as_ref() else {
        return;
    };
    let defined: BTreeSet<String> = project.input_actions().into_iter().collect();
    for script in &context.scripts {
        for (line_number, line) in script.lines() {
            for call in INPUT_CALLS {
                let Some(action) = first_quoted_after(line, call) else {
                    continue;
                };
                if action.starts_with(BUILT_IN_INPUT_PREFIX) || defined.contains(&action) {
                    continue;
                }
                collect(
                    findings,
                    Finding::draft(
                        InspectorId::Gameplay,
                        CODE_UNDEFINED_INPUT,
                        Severity::Critical,
                        INSPECT_CONFIDENCE_CERTAIN,
                        format!("Input action `{action}` is read but never defined"),
                        Location::line(&script.rel, line_number).with_symbol(action.clone()),
                    )
                    .cause(format!(
                        "Line {line_number} reads `{action}`, and project.godot's input map \
                         does not define it."
                    ))
                    .impact(
                        "Godot raises \"the InputMap action does not exist\" at run time and \
                         the check is always false — the control simply does nothing, on \
                         every platform, forever.",
                    )
                    .recommend(format!(
                        "Add `{action}` to Project Settings → Input Map with the key it \
                         should be on, or use the action that already exists."
                    ))
                    .evidence(
                        line.trim().chars().take(120).collect::<String>(),
                        format!("{}:{line_number}", script.rel),
                    )
                    .evidence(
                        format!(
                            "{} action(s) defined, none named \"{action}\"",
                            defined.len()
                        ),
                        "project.godot#input".to_owned(),
                    ),
                );
            }
        }
    }
}

/// A signal a script declares and emits, that nothing connects to.
fn signals(context: &InspectContext<'_>, findings: &mut Vec<Finding>) {
    if !context.is_project_scope() {
        return;
    }
    for script in &context.scripts {
        let declared: BTreeSet<String> = script
            .lines()
            .filter_map(|(_, line)| {
                let rest = line.trim_start().strip_prefix("signal ")?;
                let name = rest
                    .split(['(', ' ', '#'])
                    .next()
                    .map(str::trim)
                    .filter(|name| !name.is_empty())?;
                Some(name.to_owned())
            })
            .collect();
        for (line_number, line) in script.lines() {
            let emitted = first_quoted_after(line, "emit_signal(").or_else(|| {
                declared
                    .iter()
                    .find(|name| line.trim_start().starts_with(&format!("{name}.emit(")))
                    .cloned()
            });
            let Some(name) = emitted else { continue };
            if !declared.contains(&name) || listened_to(context, &name) {
                continue;
            }
            collect(
                findings,
                Finding::draft(
                    InspectorId::Gameplay,
                    CODE_UNHEARD_SIGNAL,
                    Severity::Medium,
                    INSPECT_CONFIDENCE_HEURISTIC,
                    format!("Signal `{name}` is emitted and nothing listens"),
                    Location::line(&script.rel, line_number).with_symbol(name.clone()),
                )
                .cause(format!(
                    "`{name}` is declared and emitted in {}, and no scene connection and no \
                     `connect` call anywhere in the project names it.",
                    script.rel
                ))
                .impact(
                    "The emit costs nothing and does nothing. Whatever was supposed to react \
                     — a score, a door, a sound — never does.",
                )
                .recommend(format!(
                    "Connect `{name}` where it should be handled, or remove the signal if \
                     the design moved on."
                ))
                .evidence(
                    line.trim().chars().take(120).collect::<String>(),
                    format!("{}:{line_number}", script.rel),
                ),
            );
        }
    }
}

fn listened_to(context: &InspectContext<'_>, signal: &str) -> bool {
    let in_scenes = context.snapshot.scenes.iter().any(|entry| {
        entry.parsed().is_some_and(|scene| {
            scene
                .document
                .connections
                .iter()
                .any(|connection| connection.signal == signal)
        })
    });
    in_scenes
        || context
            .snapshot
            .scripts
            .iter()
            .any(|script| script_connects(&script.source, signal))
}

/// `(script path, source)` of the nearest ancestor of `node` — itself included — that has a
/// script this scan read.
fn scripted_owner(
    context: &InspectContext<'_>,
    entry: &SceneEntry,
    node: &NodeRef<'_>,
) -> Option<(String, String)> {
    let scene = entry.parsed()?;
    let mut path = node.path.to_owned();
    loop {
        if let Some(found) = nodes(scene).iter().find(|candidate| candidate.path == path) {
            if let Some(res) = found.script(scene) {
                let rel = crate::godot::res_to_rel(&res);
                if let Some(script) = context.snapshot.script(&rel) {
                    return Some((script.rel.clone(), script.source.clone()));
                }
            }
        }
        let Some((parent, _)) = path.rsplit_once('/') else {
            // One more hop: the root, addressed as ".".
            if path == "." {
                return None;
            }
            path = ".".to_owned();
            continue;
        };
        path = parent.to_owned();
    }
}

/// The node path a connection should target: the nearest scripted ancestor.
fn script_owner_path(
    scene: &crate::godot::scene::GodotScene,
    node: &NodeRef<'_>,
) -> Option<String> {
    let mut path = node.path.to_owned();
    loop {
        if let Some(found) = nodes(scene).iter().find(|candidate| candidate.path == path) {
            if found.script(scene).is_some() {
                return Some(path);
            }
        }
        let Some((parent, _)) = path.rsplit_once('/') else {
            if path == "." {
                return None;
            }
            path = ".".to_owned();
            continue;
        };
        path = parent.to_owned();
    }
}

/// True when a script wires this signal itself.
#[must_use]
pub fn script_connects(source: &str, signal: &str) -> bool {
    source.contains(&format!("{signal}.connect("))
        || source.contains(&format!("connect(\"{signal}\""))
        || source.contains(&format!("connect('{signal}'"))
}

/// The handler a script declares for this signal, by Godot's own naming convention.
#[must_use]
pub fn handler_for(source: &str, signal: &str) -> Option<String> {
    let suffix = format!("_{signal}");
    func_bodies(source)
        .into_iter()
        .map(|(name, _, _)| name)
        .find(|name| name.ends_with(&suffix) && name.starts_with("_on"))
}

/// The first double- or single-quoted string after `needle` on one line.
#[must_use]
pub fn first_quoted_after(line: &str, needle: &str) -> Option<String> {
    let start = line.find(needle)? + needle.len();
    let rest = &line[start..];
    let quote = rest
        .chars()
        .find(|character| *character == '"' || *character == '\'')?;
    let open = rest.find(quote)? + 1;
    let after = &rest[open..];
    // Anything before the quote that is not whitespace means the argument is not a literal.
    if rest[..open - 1].trim().is_empty() {
        let end = after.find(quote)?;
        let value = &after[..end];
        (!value.is_empty()).then(|| value.to_owned())
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inspect::context::test_support::{context_from, scene_entry, script_entry};
    use crate::inspect::snapshot::ProjectSnapshot;

    const DOOR_SCENE: &str = r#"[gd_scene load_steps=2 format=3]

[ext_resource type="Script" path="res://scripts/door.gd" id="1_door"]

[node name="Door" type="StaticBody3D"]
script = ExtResource("1_door")

[node name="Trigger" type="Area3D" parent="."]
"#;

    const DOOR_SCRIPT: &str = "extends StaticBody3D\n\nfunc _on_trigger_body_entered(body):\n\tstart_interaction()\n\nfunc start_interaction():\n\tpass\n";

    fn door_project() -> ProjectSnapshot {
        ProjectSnapshot {
            main_scene: Some("scenes/door.tscn".to_owned()),
            files: ["scenes/door.tscn".to_owned(), "scripts/door.gd".to_owned()]
                .into_iter()
                .collect(),
            scenes: vec![scene_entry("scenes/door.tscn", DOOR_SCENE)],
            scripts: vec![script_entry("scripts/door.gd", DOOR_SCRIPT)],
            ..Default::default()
        }
    }

    #[test]
    fn an_overlap_handler_with_no_connection_is_reported_with_the_fix_that_connects_it() {
        let snapshot = door_project();
        let output = inspect(&context_from(&snapshot));
        let trigger = output
            .findings
            .iter()
            .find(|finding| finding.code == CODE_TRIGGER_UNWIRED)
            .expect("the unwired trigger is reported");
        assert_eq!(trigger.severity, Severity::High);
        assert_eq!(trigger.confidence, 97);
        assert_eq!(trigger.location.node.as_deref(), Some("Trigger"));

        let fix = trigger.fix.as_ref().expect("a fix is proposed");
        assert_eq!(fix.actions.len(), 1);
        match &fix.actions[0] {
            GodotAction::ConnectSignal {
                from,
                signal,
                to,
                method,
                ..
            } => {
                assert_eq!(from, "Trigger");
                assert_eq!(signal, "body_entered");
                assert_eq!(to, ".");
                assert_eq!(method, "_on_trigger_body_entered");
            }
            other => panic!("expected a ConnectSignal, got {other:?}"),
        }
    }

    #[test]
    fn the_same_trigger_wired_in_the_scene_is_not_reported() {
        let wired = format!(
            "{DOOR_SCENE}\n[connection signal=\"body_entered\" from=\"Trigger\" to=\".\" method=\"_on_trigger_body_entered\"]\n"
        );
        let snapshot = ProjectSnapshot {
            scenes: vec![scene_entry("scenes/door.tscn", &wired)],
            scripts: vec![script_entry("scripts/door.gd", DOOR_SCRIPT)],
            files: ["scenes/door.tscn".to_owned(), "scripts/door.gd".to_owned()]
                .into_iter()
                .collect(),
            ..Default::default()
        };
        let output = inspect(&context_from(&snapshot));
        assert!(!output
            .findings
            .iter()
            .any(|finding| finding.code == CODE_TRIGGER_UNWIRED
                && finding.title.contains("body_entered")));
    }

    #[test]
    fn a_trigger_the_script_connects_at_run_time_is_wired_too() {
        let script = format!("{DOOR_SCRIPT}\nfunc _ready():\n\t$Trigger.body_entered.connect(_on_trigger_body_entered)\n");
        let snapshot = ProjectSnapshot {
            scenes: vec![scene_entry("scenes/door.tscn", DOOR_SCENE)],
            scripts: vec![script_entry("scripts/door.gd", &script)],
            files: ["scenes/door.tscn".to_owned(), "scripts/door.gd".to_owned()]
                .into_iter()
                .collect(),
            ..Default::default()
        };
        let output = inspect(&context_from(&snapshot));
        assert!(!output
            .findings
            .iter()
            .any(|finding| finding.code == CODE_TRIGGER_UNWIRED
                && finding.title.contains("body_entered")));
    }

    #[test]
    fn a_group_the_game_asks_for_and_nothing_joins_is_a_progression_blocker() {
        let snapshot = ProjectSnapshot {
            scenes: vec![scene_entry(
                "scenes/level.tscn",
                "[gd_scene format=3]\n\n[node name=\"Level\" type=\"Node3D\"]\n\n[node name=\"Coin\" type=\"Area3D\" parent=\".\" groups=[\"pickups\"]]\n",
            )],
            scripts: vec![script_entry(
                "scripts/mission.gd",
                "extends Node\n\nfunc check():\n\tvar left = get_tree().get_nodes_in_group(\"medicine\")\n\tvar coins = get_tree().get_nodes_in_group(\"pickups\")\n",
            )],
            ..Default::default()
        };
        let output = inspect(&context_from(&snapshot));
        let empty: Vec<&str> = output
            .findings
            .iter()
            .filter(|finding| finding.code == CODE_EMPTY_GROUP)
            .filter_map(|finding| finding.location.symbol.as_deref())
            .collect();
        assert_eq!(empty, vec!["medicine"]);
    }

    #[test]
    fn an_input_action_the_project_never_defined_is_critical_and_ui_actions_are_not() {
        let project = crate::godot::project::GodotProjectFile::parse(
            "[application]\n\nconfig/name=\"Demo\"\n\n[input]\n\njump={\n\"deadzone\": 0.5,\n\"events\": []\n}\n",
        )
        .expect("the fixture project file parses");
        let snapshot = ProjectSnapshot {
            project_file: Some(project),
            scripts: vec![script_entry(
                "scripts/player.gd",
                "extends CharacterBody3D\n\nfunc _process(_d):\n\tif Input.is_action_pressed(\"jump\"):\n\t\tpass\n\tif Input.is_action_just_pressed(\"interact\"):\n\t\tpass\n\tif Input.is_action_pressed(\"ui_accept\"):\n\t\tpass\n",
            )],
            ..Default::default()
        };
        let output = inspect(&context_from(&snapshot));
        let undefined: Vec<&str> = output
            .findings
            .iter()
            .filter(|finding| finding.code == CODE_UNDEFINED_INPUT)
            .filter_map(|finding| finding.location.symbol.as_deref())
            .collect();
        assert_eq!(undefined, vec!["interact"]);
    }

    #[test]
    fn a_project_with_gameplay_and_no_player_says_so_once() {
        let snapshot = ProjectSnapshot {
            main_scene: Some("scenes/level.tscn".to_owned()),
            scenes: vec![scene_entry(
                "scenes/level.tscn",
                "[gd_scene format=3]\n\n[node name=\"Level\" type=\"Node3D\"]\n",
            )],
            scripts: vec![script_entry(
                "scripts/game.gd",
                "extends Node\n\nfunc _process(_d):\n\tif Input.is_action_pressed(\"ui_accept\"):\n\t\tpass\n",
            )],
            ..Default::default()
        };
        let output = inspect(&context_from(&snapshot));
        let players: Vec<_> = output
            .findings
            .iter()
            .filter(|finding| finding.code == CODE_NO_PLAYER)
            .collect();
        assert_eq!(players.len(), 1);
        assert_eq!(players[0].severity, Severity::Critical);
    }

    /// An empty project is empty, not broken. Caught by the scaffold guard in
    /// `tests/inspect_end_to_end.rs`, which is the whole reason that guard exists.
    #[test]
    fn a_project_that_is_not_a_game_yet_is_not_told_it_has_no_player() {
        let snapshot = ProjectSnapshot {
            main_scene: Some("scenes/main.tscn".to_owned()),
            scenes: vec![scene_entry(
                "scenes/main.tscn",
                "[gd_scene format=3]\n\n[node name=\"Main\" type=\"Node3D\"]\n",
            )],
            scripts: vec![script_entry(
                "scripts/main.gd",
                "extends Node3D\n\nfunc _ready():\n\tpass\n",
            )],
            ..Default::default()
        };
        let output = inspect(&context_from(&snapshot));
        assert!(!output
            .findings
            .iter()
            .any(|finding| finding.code == CODE_NO_PLAYER));
    }

    #[test]
    fn the_first_quoted_argument_is_only_read_when_it_really_is_the_argument() {
        assert_eq!(
            first_quoted_after(
                "if Input.is_action_pressed(\"jump\"):",
                "is_action_pressed("
            ),
            Some("jump".to_owned())
        );
        // A variable argument is not a literal, and must not be reported as one.
        assert_eq!(
            first_quoted_after(
                "if Input.is_action_pressed(action_name):",
                "is_action_pressed("
            ),
            None
        );
    }
}
