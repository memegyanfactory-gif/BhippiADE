//! `/inspect …`, parsed (ADR-0056 §10, §30).
//!
//! The command surface is deliberately tiny and deliberately in Rust: the webview computes
//! nothing (R3), so what "inspect this" means — which scope, which specialists, which
//! severities — is decided here and tested here, and the same parse serves the chat command,
//! the top-bar menu and the CLI.
//!
//! Unknown words are an error rather than a silent whole-project scan. `/inspect scnee` that
//! quietly scanned everything would be indistinguishable from one that worked.

use bhippi_types::{InspectorId, Severity};

use crate::error::{EngineError, Result};

use super::report::InspectScope;

/// What the user has selected in the studio when they ask to inspect "this".
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Selection {
    pub scene: Option<String>,
    pub node: Option<String>,
    pub file: Option<String>,
    pub asset: Option<String>,
}

impl Selection {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.scene.is_none() && self.node.is_none() && self.file.is_none() && self.asset.is_none()
    }
}

/// One resolved request.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InspectCommand {
    pub scope: InspectScope,
    /// Which specialists to run. Empty means every one of them.
    pub inspectors: Vec<InspectorId>,
    /// Report only findings at least this bad. `None` reports everything.
    pub min_severity: Option<Severity>,
}

impl InspectCommand {
    /// True when this inspector should run.
    #[must_use]
    pub fn runs(&self, inspector: InspectorId) -> bool {
        self.inspectors.is_empty() || self.inspectors.contains(&inspector)
    }
}

/// What the caller knows about the studio when the command was typed.
#[derive(Clone, Debug, Default)]
pub struct CommandContext {
    /// The level open in the viewport, project-relative.
    pub current_level: Option<String>,
    pub selection: Selection,
    /// The files a caller has determined changed. The engine never runs git.
    pub changed_files: Vec<String>,
}

/// Parse `/inspect`, `/inspect scene`, `/inspect selected`, `/inspect critical`, …
///
/// # Errors
/// When the word after `/inspect` is not a scope, an inspector or a severity, and when a
/// scope the studio cannot satisfy is asked for — inspecting the selection with nothing
/// selected is a mistake worth naming rather than widening into a project scan.
pub fn parse(text: &str, context: &CommandContext) -> Result<InspectCommand> {
    let trimmed = text.trim();
    let rest = trimmed
        .strip_prefix("/inspect")
        .ok_or_else(|| {
            EngineError::Action(
                format!("{trimmed} is not an inspect command"),
                Some("Use `/inspect [scope|inspector]`.".to_owned()),
            )
        })?
        .trim();

    if rest.is_empty() {
        return Ok(InspectCommand {
            scope: InspectScope::Project,
            inspectors: Vec::new(),
            min_severity: None,
        });
    }

    let mut inspectors = Vec::new();
    let mut min_severity = None;
    let mut scope = None;

    for word in rest.split_whitespace() {
        let lowered = word.to_ascii_lowercase();
        match lowered.as_str() {
            "project" | "all" | "everything" => scope = Some(InspectScope::Project),
            "selected" | "selection" | "this" => {
                if context.selection.is_empty() {
                    return Err(EngineError::Action(
                        "nothing is selected".to_owned(),
                        Some(
                            "Select a node, a script or an asset in the studio first, or use \
                             `/inspect` for the whole project."
                                .to_owned(),
                        ),
                    ));
                }
                scope = Some(InspectScope::Selection {
                    scene: context.selection.scene.clone(),
                    node: context.selection.node.clone(),
                    file: context.selection.file.clone(),
                    asset: context.selection.asset.clone(),
                });
            }
            "level" | "current" => {
                let Some(level) = context.current_level.clone() else {
                    return Err(EngineError::Action(
                        "no level is open".to_owned(),
                        Some(
                            "Open a scene in the viewport first, or use `/inspect` for the \
                             whole project."
                                .to_owned(),
                        ),
                    ));
                };
                scope = Some(InspectScope::Level { scene: level });
            }
            "changes" | "changed" | "diff" => {
                if context.changed_files.is_empty() {
                    return Err(EngineError::Action(
                        "nothing has changed".to_owned(),
                        Some(
                            "There is nothing to compare against. Use `/inspect` for the \
                             whole project."
                                .to_owned(),
                        ),
                    ));
                }
                scope = Some(InspectScope::Changes {
                    files: context.changed_files.clone(),
                });
            }
            "critical" => min_severity = Some(Severity::Critical),
            "high" => min_severity = Some(Severity::High),
            _ => {
                let Some(inspector) = InspectorId::parse(&lowered) else {
                    return Err(EngineError::Action(
                        format!("`{word}` is not something the Inspector knows how to scan"),
                        Some(format!(
                            "Try one of: {}, or a scope: project, level, selected, changes, \
                             critical.",
                            InspectorId::ALL
                                .iter()
                                .map(|inspector| inspector.as_str())
                                .collect::<Vec<_>>()
                                .join(", ")
                        )),
                    ));
                };
                if !inspectors.contains(&inspector) {
                    inspectors.push(inspector);
                }
            }
        }
    }

    Ok(InspectCommand {
        scope: scope.unwrap_or(InspectScope::Project),
        inspectors,
        min_severity,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn context() -> CommandContext {
        CommandContext {
            current_level: Some("scenes/main.tscn".to_owned()),
            selection: Selection {
                scene: Some("scenes/main.tscn".to_owned()),
                node: Some("Door".to_owned()),
                ..Selection::default()
            },
            changed_files: vec!["scripts/door.gd".to_owned()],
        }
    }

    #[test]
    fn bare_inspect_scans_the_whole_project_with_every_specialist() {
        let command = parse("/inspect", &context()).expect("a bare command parses");
        assert_eq!(command.scope, InspectScope::Project);
        assert!(command.inspectors.is_empty());
        for inspector in InspectorId::ALL {
            assert!(command.runs(inspector));
        }
    }

    #[test]
    fn an_inspector_name_narrows_the_run_to_that_inspector() {
        let command = parse("/inspect performance", &context()).expect("the command parses");
        assert_eq!(command.inspectors, vec![InspectorId::Performance]);
        assert!(command.runs(InspectorId::Performance));
        assert!(!command.runs(InspectorId::Code));
    }

    #[test]
    fn two_inspectors_and_a_severity_compose() {
        let command = parse("/inspect code physics critical", &context()).expect("it parses");
        assert_eq!(
            command.inspectors,
            vec![InspectorId::Code, InspectorId::Physics]
        );
        assert_eq!(command.min_severity, Some(Severity::Critical));
        assert_eq!(command.scope, InspectScope::Project);
    }

    #[test]
    fn the_scopes_resolve_from_what_the_studio_knows() {
        let level = parse("/inspect level", &context()).expect("it parses");
        assert_eq!(
            level.scope,
            InspectScope::Level {
                scene: "scenes/main.tscn".to_owned()
            }
        );

        let selected = parse("/inspect selected", &context()).expect("it parses");
        match selected.scope {
            InspectScope::Selection { node, .. } => assert_eq!(node.as_deref(), Some("Door")),
            other => panic!("expected a Selection scope, got {other:?}"),
        }

        let changes = parse("/inspect changes", &context()).expect("it parses");
        assert_eq!(
            changes.scope,
            InspectScope::Changes {
                files: vec!["scripts/door.gd".to_owned()]
            }
        );
    }

    #[test]
    fn a_scope_the_studio_cannot_satisfy_is_an_error_and_not_a_silent_project_scan() {
        let empty = CommandContext::default();
        let selected = parse("/inspect selected", &empty).expect_err("nothing is selected");
        assert!(selected.to_string().contains("nothing is selected"));
        assert!(selected.hint().is_some());

        assert!(parse("/inspect level", &empty).is_err());
        assert!(parse("/inspect changes", &empty).is_err());
    }

    #[test]
    fn a_word_the_inspector_does_not_know_is_refused_with_the_list_of_words_it_does() {
        let error = parse("/inspect scnee", &context()).expect_err("a typo is refused");
        let hint = error.hint().unwrap_or_default();
        assert!(
            hint.contains("gameplay"),
            "the hint lists the inspectors: {hint}"
        );
    }

    #[test]
    fn something_that_is_not_an_inspect_command_at_all_is_refused() {
        assert!(parse("/gamedebug", &context()).is_err());
    }
}
