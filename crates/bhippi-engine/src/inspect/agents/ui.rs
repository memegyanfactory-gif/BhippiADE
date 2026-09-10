//! The UI Inspector: `Control` trees, read the way a person reads a screen (ADR-0056 §2).
//!
//! Godot's UI is a tree of `Control` nodes with explicit theme overrides, which makes three
//! things checkable that are guesswork in most engines: whether a button is reachable from
//! the keyboard, whether a container's padding is symmetric, and whether a label's colour has
//! enough contrast against the panel behind it.
//!
//! The contrast check compares against the *nearest ancestor that actually declares a
//! background colour*. Where no ancestor declares one, the check says nothing — the colour
//! then comes from the theme, and a theme this crate has not resolved is not evidence.

use bhippi_types::{
    FixRisk, InspectorId, Severity, INSPECT_CONFIDENCE_CERTAIN, INSPECT_CONFIDENCE_HEURISTIC,
    INSPECT_CONTRAST_FLOOR,
};

use super::collect;
use crate::godot::action::GodotAction;
use crate::godot::scene::GodotScene;
use crate::godot::tscn::TscnValue;
use crate::inspect::context::{InspectContext, InspectorOutput};
use crate::inspect::finding::{Finding, Location};
use crate::inspect::fix::ProposedFix;
use crate::inspect::nodes::{contrast_ratio, nodes, NodeRef};

/// A control with nothing in it and nothing on it.
pub const CODE_EMPTY_CONTROL: &str = "BHP-INS-601";
/// A container whose padding is not symmetric.
pub const CODE_UNEVEN_PADDING: &str = "BHP-INS-602";
/// Text that cannot be read against what is behind it.
pub const CODE_LOW_CONTRAST: &str = "BHP-INS-603";
/// An interactive control the keyboard cannot reach.
pub const CODE_NOT_FOCUSABLE: &str = "BHP-INS-604";

/// The four margin constants a `MarginContainer` carries.
const MARGINS: [&str; 4] = [
    "theme_override_constants/margin_left",
    "theme_override_constants/margin_top",
    "theme_override_constants/margin_right",
    "theme_override_constants/margin_bottom",
];

/// Controls a person clicks or types into.
const INTERACTIVE: [&str; 7] = [
    "Button",
    "CheckBox",
    "CheckButton",
    "OptionButton",
    "LineEdit",
    "TextEdit",
    "MenuButton",
];

/// `FOCUS_NONE` in Godot's `Control.FocusMode`.
const FOCUS_NONE: i64 = 0;

#[must_use]
pub fn inspect(context: &InspectContext<'_>) -> InspectorOutput {
    let mut findings = Vec::new();

    for entry in &context.scenes {
        let Some(scene) = entry.parsed() else {
            continue;
        };
        let all = nodes(scene);
        for node in &all {
            empty_control(entry.rel.as_str(), scene, node, &all, &mut findings);
            uneven_padding(entry.rel.as_str(), node, &mut findings);
            low_contrast(entry.rel.as_str(), node, &all, &mut findings);
            not_focusable(entry.rel.as_str(), node, &mut findings);
        }
    }

    InspectorOutput {
        findings,
        coverage: context.scene_coverage(),
    }
}

/// A button or label with no text, no icon and no children draws nothing.
fn empty_control(
    scene_rel: &str,
    _scene: &GodotScene,
    node: &NodeRef<'_>,
    all: &[NodeRef<'_>],
    findings: &mut Vec<Finding>,
) {
    if !node.is_type(&["Button", "Label", "RichTextLabel", "CheckBox"]) {
        return;
    }
    let has_text = node
        .str_prop("text")
        .is_some_and(|text| !text.trim().is_empty());
    let has_icon = node.get("icon").is_some() || node.get("texture").is_some();
    let prefix = format!("{}/", node.path);
    let has_children = all
        .iter()
        .any(|other| other.path.starts_with(prefix.as_str()));
    if has_text || has_icon || has_children {
        return;
    }
    collect(
        findings,
        Finding::draft(
            InspectorId::Ui,
            CODE_EMPTY_CONTROL,
            Severity::Medium,
            INSPECT_CONFIDENCE_HEURISTIC,
            format!("`{}` has nothing to show", node.path),
            Location::node(scene_rel, node.path),
        )
        .cause(format!(
            "The {} has no `text`, no icon or texture, and no child nodes.",
            node.type_.unwrap_or("Control")
        ))
        .impact(
            "It occupies layout space and draws nothing, so the screen has an invisible gap \
             — or an invisible button, which is worse.",
        )
        .recommend(
            "Give it its text or icon, or delete it if the layout no longer needs it. If a \
             script sets the text at run time, confirm that script actually runs.",
        )
        .evidence(
            "no text, icon or children".to_owned(),
            format!("{scene_rel}#{}", node.path),
        ),
    );
}

/// A container whose left and right (or top and bottom) padding differ.
fn uneven_padding(scene_rel: &str, node: &NodeRef<'_>, findings: &mut Vec<Finding>) {
    if !node.is_type(&["MarginContainer"]) {
        return;
    }
    let values: Vec<(usize, i64)> = MARGINS
        .iter()
        .enumerate()
        .filter_map(|(index, margin)| node.int_prop(margin).map(|value| (index, value)))
        .collect();
    // Fewer than two set means the theme decides; that is not an inconsistency in the scene.
    if values.len() < 2 {
        return;
    }
    let horizontal: Vec<i64> = values
        .iter()
        .filter(|(index, _)| *index == 0 || *index == 2)
        .map(|(_, value)| *value)
        .collect();
    let vertical: Vec<i64> = values
        .iter()
        .filter(|(index, _)| *index == 1 || *index == 3)
        .map(|(_, value)| *value)
        .collect();
    let uneven_horizontal = horizontal.len() == 2 && horizontal[0] != horizontal[1];
    let uneven_vertical = vertical.len() == 2 && vertical[0] != vertical[1];
    if !uneven_horizontal && !uneven_vertical {
        return;
    }

    let (axis, pair, first, second) = if uneven_horizontal {
        (
            "horizontal",
            ("left", "right"),
            horizontal[0],
            horizontal[1],
        )
    } else {
        ("vertical", ("top", "bottom"), vertical[0], vertical[1])
    };
    let target = first.max(second);
    let lower_property = if first < second {
        MARGINS[if uneven_horizontal { 0 } else { 1 }]
    } else {
        MARGINS[if uneven_horizontal { 2 } else { 3 }]
    };

    let mut draft = Finding::draft(
        InspectorId::Ui,
        CODE_UNEVEN_PADDING,
        Severity::Low,
        INSPECT_CONFIDENCE_CERTAIN,
        format!("`{}` padding is {first} / {second}", node.path),
        Location::node(scene_rel, node.path),
    )
    .cause(format!(
        "The {axis} margins differ: {} is {first} and {} is {second}.",
        pair.0, pair.1
    ))
    .impact(
        "The content sits off-centre by the difference. It is small enough that nobody \
         reports it and large enough that the screen looks unfinished.",
    )
    .recommend(format!(
        "Set both {axis} margins to {target}, unless the asymmetry is deliberate."
    ))
    .evidence(
        format!("{} = {first}, {} = {second}", pair.0, pair.1),
        format!("{scene_rel}#{}", node.path),
    );

    match ProposedFix::new(
        CODE_UNEVEN_PADDING,
        format!(
            "Set `{}` {axis} padding to {target} on both sides",
            node.path
        ),
        FixRisk::Low,
        vec![GodotAction::SetProperty {
            scene: scene_rel.to_owned(),
            path: node.path.to_owned(),
            property: lower_property.to_owned(),
            value: TscnValue::Int(target),
        }],
    ) {
        Ok(fix) => draft = draft.fix(fix),
        Err(error) => tracing::error!(%error, "the padding fix could not be described"),
    }
    collect(findings, draft);
}

/// Text against a background it cannot be read on.
fn low_contrast(
    scene_rel: &str,
    node: &NodeRef<'_>,
    all: &[NodeRef<'_>],
    findings: &mut Vec<Finding>,
) {
    let Some((red, green, blue, alpha)) = node.color_prop("theme_override_colors/font_color")
    else {
        return;
    };
    // A transparent colour is a different problem, and not this check's.
    if alpha < 1.0 {
        return;
    }
    let Some((background, source)) = nearest_background(node, all) else {
        return;
    };
    let ratio = contrast_ratio((red, green, blue), background);
    if ratio >= INSPECT_CONTRAST_FLOOR {
        return;
    }
    let rounded = (ratio * 10.0).round() / 10.0;
    collect(
        findings,
        Finding::draft(
            InspectorId::Ui,
            CODE_LOW_CONTRAST,
            Severity::Medium,
            INSPECT_CONFIDENCE_CERTAIN,
            format!("`{}` text contrast is {rounded}:1", node.path),
            Location::node(scene_rel, node.path),
        )
        .cause(format!(
            "Its font colour against `{source}`'s background gives {rounded}:1, under the \
             {INSPECT_CONTRAST_FLOOR}:1 floor for body text."
        ))
        .impact(
            "It is hard to read on a bright screen and unreadable for a player with low \
             vision — and it is the studio's own accessibility floor (INV-034).",
        )
        .recommend(format!(
            "Lighten or darken the text until it reaches {INSPECT_CONTRAST_FLOOR}:1 against \
             that background, or change the background behind it."
        ))
        .evidence(
            format!("contrast {rounded}:1 against `{source}`"),
            format!("{scene_rel}#{}", node.path),
        ),
    );
}

/// The nearest ancestor (or the node itself) that declares a background colour.
fn nearest_background<'a>(
    node: &NodeRef<'a>,
    all: &[NodeRef<'a>],
) -> Option<((f64, f64, f64), String)> {
    let mut path = node.path.to_owned();
    loop {
        if let Some(found) = all.iter().find(|candidate| candidate.path == path) {
            if let Some((red, green, blue, alpha)) = found
                .color_prop("color")
                .or_else(|| found.color_prop("theme_override_colors/background_color"))
            {
                if alpha >= 1.0 {
                    // The root's path is "."; its name is what a person would recognise.
                    let named = if found.path == "." {
                        found.name.to_owned()
                    } else {
                        found.path.to_owned()
                    };
                    return Some(((red, green, blue), named));
                }
            }
        }
        match path.rsplit_once('/') {
            Some((parent, _)) => path = parent.to_owned(),
            None if path != "." => path = ".".to_owned(),
            None => return None,
        }
    }
}

/// A button the keyboard cannot reach.
fn not_focusable(scene_rel: &str, node: &NodeRef<'_>, findings: &mut Vec<Finding>) {
    if !node.type_ends_with(&INTERACTIVE) {
        return;
    }
    if node.int_prop("focus_mode") != Some(FOCUS_NONE) {
        return;
    }
    let mut draft = Finding::draft(
        InspectorId::Ui,
        CODE_NOT_FOCUSABLE,
        Severity::Medium,
        INSPECT_CONFIDENCE_CERTAIN,
        format!("`{}` cannot be reached from the keyboard", node.path),
        Location::node(scene_rel, node.path),
    )
    .cause("`focus_mode = 0` (FOCUS_NONE), so Tab and the D-pad skip over it.")
    .impact(
        "Anyone playing with a keyboard or a controller cannot activate it at all, and the \
         focus order silently jumps past it for everyone else.",
    )
    .recommend("Set focus_mode to All (2) unless the control is genuinely decorative.")
    .evidence(
        "focus_mode = 0".to_owned(),
        format!("{scene_rel}#{}", node.path),
    );

    match ProposedFix::new(
        CODE_NOT_FOCUSABLE,
        format!("Make `{}` focusable", node.path),
        FixRisk::Low,
        vec![GodotAction::SetProperty {
            scene: scene_rel.to_owned(),
            path: node.path.to_owned(),
            property: "focus_mode".to_owned(),
            value: TscnValue::Int(2),
        }],
    ) {
        Ok(fix) => draft = draft.fix(fix),
        Err(error) => tracing::error!(%error, "the focus fix could not be described"),
    }
    collect(findings, draft);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inspect::context::test_support::{context_from, scene_entry};
    use crate::inspect::snapshot::ProjectSnapshot;

    fn from_scene(text: &str) -> Vec<Finding> {
        let snapshot = ProjectSnapshot {
            scenes: vec![scene_entry("scenes/hud.tscn", text)],
            ..Default::default()
        };
        inspect(&context_from(&snapshot)).findings
    }

    #[test]
    fn uneven_padding_is_reported_with_the_fix_that_evens_it_up() {
        let findings = from_scene(
            r#"[gd_scene format=3]

[node name="HUD" type="Control"]

[node name="Inventory" type="MarginContainer" parent="."]
theme_override_constants/margin_left = 24
theme_override_constants/margin_right = 12
"#,
        );
        let padding = findings
            .iter()
            .find(|finding| finding.code == CODE_UNEVEN_PADDING)
            .expect("the uneven padding is reported");
        assert!(padding.title.contains("24 / 12"));
        let fix = padding.fix.as_ref().expect("a fix is proposed");
        match &fix.actions[0] {
            GodotAction::SetProperty {
                property, value, ..
            } => {
                assert_eq!(property, "theme_override_constants/margin_right");
                assert_eq!(*value, TscnValue::Int(24));
            }
            other => panic!("expected SetProperty, got {other:?}"),
        }
    }

    #[test]
    fn symmetric_padding_and_theme_driven_padding_are_both_silent() {
        let even = from_scene(
            r#"[gd_scene format=3]

[node name="HUD" type="Control"]

[node name="Inventory" type="MarginContainer" parent="."]
theme_override_constants/margin_left = 24
theme_override_constants/margin_right = 24
"#,
        );
        assert!(!even
            .iter()
            .any(|finding| finding.code == CODE_UNEVEN_PADDING));

        let themed = from_scene(
            "[gd_scene format=3]\n\n[node name=\"HUD\" type=\"Control\"]\n\n[node name=\"Inventory\" type=\"MarginContainer\" parent=\".\"]\n",
        );
        assert!(!themed
            .iter()
            .any(|finding| finding.code == CODE_UNEVEN_PADDING));
    }

    #[test]
    fn a_button_the_keyboard_skips_is_reported_and_offers_to_make_it_focusable() {
        let findings = from_scene(
            r#"[gd_scene format=3]

[node name="Menu" type="Control"]

[node name="Start" type="Button" parent="."]
text = "Start"
focus_mode = 0
"#,
        );
        let focus = findings
            .iter()
            .find(|finding| finding.code == CODE_NOT_FOCUSABLE)
            .expect("the unreachable button is reported");
        let fix = focus.fix.as_ref().expect("a fix is proposed");
        assert_eq!(fix.files, vec!["scenes/hud.tscn".to_owned()]);
    }

    #[test]
    fn dark_text_on_a_dark_panel_is_reported_and_readable_text_is_not() {
        let bad = from_scene(
            r#"[gd_scene format=3]

[node name="Panel" type="ColorRect"]
color = Color(0.1, 0.1, 0.1, 1)

[node name="Score" type="Label" parent="."]
text = "0"
theme_override_colors/font_color = Color(0.2, 0.2, 0.2, 1)
"#,
        );
        let contrast = bad
            .iter()
            .find(|finding| finding.code == CODE_LOW_CONTRAST)
            .expect("the unreadable label is reported");
        assert!(contrast.cause.contains("Panel"));

        let good = from_scene(
            r#"[gd_scene format=3]

[node name="Panel" type="ColorRect"]
color = Color(0.05, 0.05, 0.05, 1)

[node name="Score" type="Label" parent="."]
text = "0"
theme_override_colors/font_color = Color(0.95, 0.95, 0.95, 1)
"#,
        );
        assert!(!good.iter().any(|finding| finding.code == CODE_LOW_CONTRAST));
    }

    #[test]
    fn a_label_with_no_text_and_no_children_is_reported_and_one_with_text_is_not() {
        let findings = from_scene(
            r#"[gd_scene format=3]

[node name="HUD" type="Control"]

[node name="Empty" type="Label" parent="."]

[node name="Score" type="Label" parent="."]
text = "0"
"#,
        );
        let empty: Vec<&str> = findings
            .iter()
            .filter(|finding| finding.code == CODE_EMPTY_CONTROL)
            .filter_map(|finding| finding.location.node.as_deref())
            .collect();
        assert_eq!(empty, vec!["Empty"]);
    }
}
