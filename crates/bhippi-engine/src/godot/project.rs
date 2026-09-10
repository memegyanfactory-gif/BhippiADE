//! `project.godot` — Godot's INI-like `ConfigFile` format.
//!
//! The same grammar backs `export_presets.cfg`, so [`GodotIniFile`] is the shared model and
//! [`GodotProjectFile`] is the typed view over it. Values reuse [`TscnValue`], because a
//! `project.godot` value *is* a Godot variant: `config/features` is a `PackedStringArray`,
//! an input action is a dictionary of `Object(InputEventKey, …)` events.
//!
//! # The layout this writes
//!
//! Godot's own writers put a blank line after every `[section]` header and one blank line
//! between sections; `project.godot` additionally carries a seven-line `;` comment banner
//! and the top-level `config_version=5` before the first section. All of that is
//! reproduced, and the banner is kept verbatim so a project the editor wrote comes back
//! byte for byte.

use super::tscn::{
    is_key_line, is_section_line, parse_section_header, parse_value, quote_text, TscnValue,
};
use crate::error::{EngineError, Result};
use serde::{Deserialize, Serialize};
use specta::Type;
use std::fmt::Write as _;

/// `config_version` for Godot 4 projects.
pub const PROJECT_CONFIG_VERSION: u32 = 5;
/// The default deadzone Godot gives a new input action.
pub const DEFAULT_INPUT_DEADZONE: f64 = 0.5;
/// The largest INI file the parser accepts.
pub const MAX_INI_BYTES: usize = 8 * 1024 * 1024;
/// The section listing the editor plugins a project turns on.
pub const EDITOR_PLUGINS_SECTION: &str = "editor_plugins";
/// The one key in that section.
pub const EDITOR_PLUGINS_KEY: &str = "enabled";

/// The banner Godot writes at the top of every `project.godot`.
pub const PROJECT_BANNER: &[&str] = &[
    "; Engine configuration file.",
    "; It's best edited using the editor UI and not directly,",
    "; since the parameters that go here are not all obvious.",
    ";",
    "; Format:",
    ";   [section] ; section goes between []",
    ";   param=value ; assign values to parameters",
];

/// One `[name]` block and its ordered entries.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize, Type)]
pub struct IniSection {
    pub name: String,
    pub entries: Vec<(String, TscnValue)>,
}

impl IniSection {
    #[must_use]
    pub fn get(&self, key: &str) -> Option<&TscnValue> {
        self.entries
            .iter()
            .find(|(name, _)| name == key)
            .map(|(_, value)| value)
    }

    pub fn set(&mut self, key: &str, value: TscnValue) {
        if let Some(slot) = self.entries.iter_mut().find(|(name, _)| name == key) {
            slot.1 = value;
        } else {
            self.entries.push((key.to_owned(), value));
        }
    }

    /// Remove one key; `true` when it was there.
    pub fn remove(&mut self, key: &str) -> bool {
        let before = self.entries.len();
        self.entries.retain(|(name, _)| name != key);
        self.entries.len() != before
    }
}

/// A parsed Godot `ConfigFile`: comment banner, top-level keys, ordered sections.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize, Type)]
pub struct GodotIniFile {
    /// Leading comment lines, verbatim and without their newlines.
    #[serde(default)]
    pub banner: Vec<String>,
    /// Keys before the first `[section]` (`config_version=5`).
    #[serde(default)]
    pub globals: Vec<(String, TscnValue)>,
    #[serde(default)]
    pub sections: Vec<IniSection>,
}

impl GodotIniFile {
    #[must_use]
    pub fn section(&self, name: &str) -> Option<&IniSection> {
        self.sections.iter().find(|section| section.name == name)
    }

    pub fn section_mut(&mut self, name: &str) -> Option<&mut IniSection> {
        self.sections
            .iter_mut()
            .find(|section| section.name == name)
    }

    /// The section, created in alphabetical position if it is missing. Godot keeps sections
    /// sorted, so inserting in place keeps a Bhippi edit from reordering the whole file the
    /// next time the editor saves.
    pub fn ensure_section(&mut self, name: &str) -> &mut IniSection {
        if let Some(index) = self
            .sections
            .iter()
            .position(|section| section.name == name)
        {
            let Some(section) = self.sections.get_mut(index) else {
                unreachable!("index came from the same vec")
            };
            return section;
        }
        let at = self
            .sections
            .iter()
            .position(|section| section.name.as_str() > name)
            .unwrap_or(self.sections.len());
        self.sections.insert(
            at,
            IniSection {
                name: name.to_owned(),
                entries: Vec::new(),
            },
        );
        let Some(section) = self.sections.get_mut(at) else {
            unreachable!("just inserted at this index")
        };
        section
    }

    #[must_use]
    pub fn get(&self, section: &str, key: &str) -> Option<&TscnValue> {
        self.section(section)?.get(key)
    }

    pub fn set(&mut self, section: &str, key: &str, value: TscnValue) {
        self.ensure_section(section).set(key, value);
    }

    /// Remove one key, and the section with it when that empties it.
    pub fn remove(&mut self, section: &str, key: &str) -> bool {
        let Some(target) = self.section_mut(section) else {
            return false;
        };
        let removed = target.remove(key);
        if removed {
            self.sections
                .retain(|section| !section.entries.is_empty() || section.name.is_empty());
        }
        removed
    }

    /// Render the file the way Godot's writers lay it out.
    #[must_use]
    pub fn to_text(&self) -> String {
        let mut out = String::new();
        if !self.banner.is_empty() {
            for line in &self.banner {
                out.push_str(line);
                out.push('\n');
            }
            out.push('\n');
        }
        for (key, value) in &self.globals {
            let _ = writeln!(out, "{key}={}", value.to_text());
        }
        let mut wrote = !self.globals.is_empty();
        for section in &self.sections {
            if wrote {
                out.push('\n');
            }
            let _ = writeln!(out, "[{}]", section.name);
            out.push('\n');
            for (key, value) in &section.entries {
                let _ = writeln!(out, "{key}={}", value.to_text());
            }
            wrote = true;
        }
        out
    }
}

/// Parse a Godot `ConfigFile` (`project.godot`, `export_presets.cfg`).
pub fn parse_ini(text: &str) -> Result<GodotIniFile> {
    if text.len() > MAX_INI_BYTES {
        return Err(EngineError::Manifest(
            format!("config file is {} bytes, past the parser cap", text.len()),
            Some(
                "Open the project in Godot; Bhippi only edits files it can hold in memory."
                    .to_owned(),
            ),
        ));
    }
    let text = text.replace("\r\n", "\n");
    let mut file = GodotIniFile::default();
    let mut in_banner = true;
    let mut current: Option<IniSection> = None;
    let mut buffered: Option<(String, String)> = None;

    fn flush(buffered: &mut Option<(String, String)>, into: &mut Vec<(String, TscnValue)>) {
        if let Some((key, raw)) = buffered.take() {
            into.push((key, parse_value(&raw)));
        }
    }

    for line in text.lines() {
        let trimmed = line.trim_end();
        if in_banner {
            if trimmed.starts_with(';') {
                file.banner.push(trimmed.to_owned());
                continue;
            }
            if trimmed.is_empty() && !file.banner.is_empty() {
                in_banner = false;
                continue;
            }
            in_banner = false;
        }
        if is_section_line(trimmed) {
            let header = parse_section_header(trimmed).ok_or_else(|| {
                EngineError::Manifest(
                    format!("malformed section line `{trimmed}`"),
                    Some("Sections look like [application].".to_owned()),
                )
            })?;
            if let Some(mut section) = current.take() {
                flush(&mut buffered, &mut section.entries);
                file.sections.push(section);
            } else {
                flush(&mut buffered, &mut file.globals);
            }
            buffered = None;
            current = Some(IniSection {
                name: header.tag,
                entries: Vec::new(),
            });
            continue;
        }
        if is_key_line(trimmed) {
            match current.as_mut() {
                Some(section) => flush(&mut buffered, &mut section.entries),
                None => flush(&mut buffered, &mut file.globals),
            }
            if let Some((key, value)) = trimmed.split_once('=') {
                buffered = Some((key.trim_end().to_owned(), value.to_owned()));
            }
            continue;
        }
        if let Some((_, raw)) = buffered.as_mut() {
            raw.push('\n');
            raw.push_str(trimmed);
        }
    }
    match current.take() {
        Some(mut section) => {
            flush(&mut buffered, &mut section.entries);
            file.sections.push(section);
        }
        None => flush(&mut buffered, &mut file.globals),
    }
    Ok(file)
}

// ── the typed project view ───────────────────────────────────────────────────────────

/// One `[autoload]` entry. Godot marks a singleton by prefixing the path with `*`.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct Autoload {
    pub name: String,
    /// The `res://…` path, without the singleton marker.
    pub path: String,
    pub singleton: bool,
}

/// `project.godot` with typed accessors for the settings Bhippi edits.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize, Type)]
pub struct GodotProjectFile {
    pub file: GodotIniFile,
}

impl GodotProjectFile {
    /// A fresh `project.godot` carrying Godot's own banner and `config_version`.
    #[must_use]
    pub fn new(name: &str, main_scene: &str, features: &[&str]) -> Self {
        let mut file = GodotIniFile {
            banner: PROJECT_BANNER
                .iter()
                .map(|line| (*line).to_owned())
                .collect(),
            globals: vec![(
                "config_version".to_owned(),
                TscnValue::Int(i64::from(PROJECT_CONFIG_VERSION)),
            )],
            sections: Vec::new(),
        };
        let application = file.ensure_section("application");
        application.set("config/name", TscnValue::str(name));
        application.set(
            "run/main_scene",
            TscnValue::str(super::rel_to_res(main_scene)),
        );
        application.set(
            "config/features",
            TscnValue::Raw(packed_string_array(features)),
        );
        Self { file }
    }

    pub fn parse(text: &str) -> Result<Self> {
        Ok(Self {
            file: parse_ini(text)?,
        })
    }

    #[must_use]
    pub fn to_text(&self) -> String {
        self.file.to_text()
    }

    /// `[application] config/name`.
    #[must_use]
    pub fn name(&self) -> Option<String> {
        self.file
            .get("application", "config/name")
            .and_then(|value| value.as_str().map(str::to_owned))
    }

    pub fn set_name(&mut self, name: &str) {
        self.file
            .set("application", "config/name", TscnValue::str(name));
    }

    /// `[application] run/main_scene`, always as a `res://` path.
    #[must_use]
    pub fn main_scene(&self) -> Option<String> {
        self.file
            .get("application", "run/main_scene")
            .and_then(|value| value.as_str().map(str::to_owned))
    }

    pub fn set_main_scene(&mut self, res_path: &str) {
        self.file.set(
            "application",
            "run/main_scene",
            TscnValue::str(super::rel_to_res(res_path)),
        );
    }

    /// `[application] config/icon`.
    #[must_use]
    pub fn icon(&self) -> Option<String> {
        self.file
            .get("application", "config/icon")
            .and_then(|value| value.as_str().map(str::to_owned))
    }

    pub fn set_icon(&mut self, res_path: &str) {
        self.file.set(
            "application",
            "config/icon",
            TscnValue::str(super::rel_to_res(res_path)),
        );
    }

    /// The strings inside `config/features=PackedStringArray("4.7", "Forward Plus")`.
    #[must_use]
    pub fn features(&self) -> Vec<String> {
        let Some(value) = self.file.get("application", "config/features") else {
            return Vec::new();
        };
        match value {
            TscnValue::Array(_) => value.as_string_list(),
            other => quoted_strings(&other.to_text()),
        }
    }

    pub fn set_features(&mut self, features: &[&str]) {
        self.file.set(
            "application",
            "config/features",
            TscnValue::Raw(packed_string_array(features)),
        );
    }

    #[must_use]
    pub fn autoloads(&self) -> Vec<Autoload> {
        let Some(section) = self.file.section("autoload") else {
            return Vec::new();
        };
        section
            .entries
            .iter()
            .filter_map(|(name, value)| {
                let raw = value.as_str()?;
                Some(Autoload {
                    name: name.clone(),
                    path: raw.strip_prefix('*').unwrap_or(raw).to_owned(),
                    singleton: raw.starts_with('*'),
                })
            })
            .collect()
    }

    /// Register an autoload. `singleton` is Godot's "Global Variable" switch, written as the
    /// `*` prefix; without it the script is loaded but not exposed under its own name.
    pub fn add_autoload(&mut self, name: &str, res_path: &str, singleton: bool) {
        let path = super::rel_to_res(res_path);
        let value = if singleton { format!("*{path}") } else { path };
        self.file.set("autoload", name, TscnValue::str(value));
    }

    pub fn remove_autoload(&mut self, name: &str) -> bool {
        self.file.remove("autoload", name)
    }

    /// The `res://…/plugin.cfg` paths in `[editor_plugins] enabled`.
    ///
    /// Godot writes this as a `PackedStringArray`, which the value parser keeps raw, so the
    /// strings are read out of the rendered text the same way `config/features` is.
    #[must_use]
    pub fn editor_plugins(&self) -> Vec<String> {
        let Some(value) = self.file.get(EDITOR_PLUGINS_SECTION, EDITOR_PLUGINS_KEY) else {
            return Vec::new();
        };
        match value {
            TscnValue::Array(_) => value.as_string_list(),
            other => quoted_strings(&other.to_text()),
        }
    }

    /// Enable an editor plugin by the `res://` path of its `plugin.cfg`. Returns `true` when
    /// the entry was added, `false` when it was already enabled.
    ///
    /// Appends rather than replaces: a project may carry plugins Bhippi knows nothing about,
    /// and turning one of those off because Bhippi turned its own on is exactly the silent
    /// damage the model in this module exists to prevent.
    pub fn enable_editor_plugin(&mut self, res_path: &str) -> bool {
        let path = super::rel_to_res(res_path);
        let mut enabled = self.editor_plugins();
        if enabled.iter().any(|existing| existing == &path) {
            return false;
        }
        enabled.push(path);
        let borrowed: Vec<&str> = enabled.iter().map(String::as_str).collect();
        self.file.set(
            EDITOR_PLUGINS_SECTION,
            EDITOR_PLUGINS_KEY,
            TscnValue::Raw(packed_string_array(&borrowed)),
        );
        true
    }

    /// The names of every action in `[input]`.
    #[must_use]
    pub fn input_actions(&self) -> Vec<String> {
        self.file
            .section("input")
            .map(|section| {
                section
                    .entries
                    .iter()
                    .map(|(name, _)| name.clone())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Add (or replace) a keyboard input action.
    ///
    /// The emitted text is Godot's own, and it is exact enough to matter — an action Godot
    /// cannot decode is an action that silently never fires. One key produces:
    ///
    /// ```text
    /// jump={
    /// "deadzone": 0.5,
    /// "events": [Object(InputEventKey,"resource_local_to_scene":false,"resource_name":"","device":-1,"window_id":0,"alt_pressed":false,"shift_pressed":false,"ctrl_pressed":false,"meta_pressed":false,"pressed":false,"keycode":32,"physical_keycode":0,"key_label":0,"unicode":0,"location":0,"echo":false,"script":null)
    /// ]
    /// }
    /// ```
    ///
    /// The trailing newline inside the array is not a typo: Godot's object writer ends every
    /// `Object(…)` with `)\n`, so a two-event array reads `[Object(…)\n, Object(…)\n]`.
    /// Properties Godot adds in a later release are simply absent here, and Godot fills them
    /// from the class defaults when it reads the file back.
    pub fn add_input_action(&mut self, name: &str, keycodes: &[u32], deadzone: f64) {
        let events: Vec<String> = keycodes
            .iter()
            .map(|keycode| format!("{}\n", input_event_key(*keycode)))
            .collect();
        let value = format!(
            "{{\n\"deadzone\": {},\n\"events\": [{}]\n}}",
            super::tscn::float_text(deadzone),
            events.join(", ")
        );
        self.file.set("input", name, TscnValue::Raw(value));
    }

    pub fn remove_input_action(&mut self, name: &str) -> bool {
        self.file.remove("input", name)
    }
}

/// One `Object(InputEventKey, …)` literal for `keycode`.
#[must_use]
pub fn input_event_key(keycode: u32) -> String {
    format!(
        "Object(InputEventKey,\"resource_local_to_scene\":false,\"resource_name\":\"\",\
         \"device\":-1,\"window_id\":0,\"alt_pressed\":false,\"shift_pressed\":false,\
         \"ctrl_pressed\":false,\"meta_pressed\":false,\"pressed\":false,\"keycode\":{keycode},\
         \"physical_keycode\":0,\"key_label\":0,\"unicode\":0,\"location\":0,\"echo\":false,\
         \"script\":null)"
    )
}

/// `PackedStringArray("a", "b")` — a form the value parser keeps raw, so it is built here.
#[must_use]
pub fn packed_string_array(items: &[&str]) -> String {
    let rendered: Vec<String> = items.iter().map(|item| quote_text(item)).collect();
    format!("PackedStringArray({})", rendered.join(", "))
}

/// Every double-quoted run inside `text`, unescaped only for `\"` and `\\`.
fn quoted_strings(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current: Option<String> = None;
    let mut escaped = false;
    for character in text.chars() {
        match current.as_mut() {
            Some(buffer) => {
                if escaped {
                    buffer.push(character);
                    escaped = false;
                } else if character == '\\' {
                    escaped = true;
                } else if character == '"' {
                    out.push(buffer.clone());
                    current = None;
                } else {
                    buffer.push(character);
                }
            }
            None => {
                if character == '"' {
                    current = Some(String::new());
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{
        packed_string_array, parse_ini, GodotProjectFile, TscnValue, EDITOR_PLUGINS_KEY,
        EDITOR_PLUGINS_SECTION,
    };

    const FIXTURE: &str = include_str!("../../../../tests/fixtures/godot/project.godot");
    /// The same file as an editor with a third-party addon enabled would have left it.
    const WITH_PLUGINS: &str =
        include_str!("../../../../tests/fixtures/godot/project-with-plugins.godot");

    fn lf(text: &str) -> String {
        text.replace("\r\n", "\n")
    }

    #[test]
    fn a_real_project_file_round_trips_byte_for_byte() {
        let source = lf(FIXTURE);
        let parsed = parse_ini(&source).expect("project.godot parses");
        assert_eq!(parsed.to_text(), source);
    }

    #[test]
    fn typed_accessors_read_what_the_editor_wrote() {
        let project = GodotProjectFile::parse(&lf(FIXTURE)).expect("parses");
        assert_eq!(project.name().as_deref(), Some("Fixture Game"));
        assert_eq!(
            project.main_scene().as_deref(),
            Some("res://scenes/main.tscn")
        );
        assert_eq!(project.features(), vec!["4.7", "Forward Plus"]);
        let autoloads = project.autoloads();
        assert_eq!(autoloads.len(), 1);
        assert_eq!(autoloads[0].name, "BhippiProbe");
        assert_eq!(autoloads[0].path, "res://bhippi/probe.gd");
        assert!(autoloads[0].singleton);
        let mut actions = project.input_actions();
        actions.sort();
        assert_eq!(actions, vec!["jump", "move_left", "move_right"]);
    }

    #[test]
    fn edits_touch_only_what_they_name() {
        let source = lf(FIXTURE);
        let mut project = GodotProjectFile::parse(&source).expect("parses");
        project.set_main_scene("scenes/level_02.tscn");
        project.add_autoload("BhippiExtra", "res://bhippi/extra.gd", true);
        let text = project.to_text();

        assert!(text.contains("run/main_scene=\"res://scenes/level_02.tscn\""));
        assert!(text.contains("BhippiExtra=\"*res://bhippi/extra.gd\""));
        // The parts nobody named are still identical, line for line.
        assert!(text.contains("window/size/viewport_width=1280"));
        assert!(text.contains("config/features=PackedStringArray(\"4.7\", \"Forward Plus\")"));

        let reparsed = GodotProjectFile::parse(&text).expect("re-parses");
        assert_eq!(reparsed.to_text(), text);
        assert_eq!(reparsed.autoloads().len(), 2);
    }

    #[test]
    fn an_input_action_is_emitted_in_godots_exact_object_syntax() {
        let mut project = GodotProjectFile::new("Demo", "scenes/main.tscn", &["4.7"]);
        project.add_input_action("jump", &[32], 0.5);
        let text = project.to_text();
        assert!(text.contains(
            "jump={\n\"deadzone\": 0.5,\n\"events\": [Object(InputEventKey,\"resource_local_to_scene\":false,\"resource_name\":\"\",\"device\":-1,\"window_id\":0,\"alt_pressed\":false,\"shift_pressed\":false,\"ctrl_pressed\":false,\"meta_pressed\":false,\"pressed\":false,\"keycode\":32,\"physical_keycode\":0,\"key_label\":0,\"unicode\":0,\"location\":0,\"echo\":false,\"script\":null)\n]\n}"
        ), "unexpected input action text:\n{text}");

        // A multi-line dictionary value survives a parse/print cycle unchanged.
        let reparsed = GodotProjectFile::parse(&text).expect("re-parses");
        assert_eq!(reparsed.to_text(), text);
        assert_eq!(reparsed.input_actions(), vec!["jump"]);
    }

    #[test]
    fn an_editor_plugins_section_round_trips_and_reads_back() {
        let source = lf(WITH_PLUGINS);
        assert_eq!(
            parse_ini(&source).expect("parses").to_text(),
            source,
            "a project.godot carrying [editor_plugins] must come back byte for byte"
        );
        let project = GodotProjectFile::parse(&source).expect("parses");
        assert_eq!(
            project.editor_plugins(),
            vec!["res://addons/someone_elses_tool/plugin.cfg"]
        );
    }

    #[test]
    fn enabling_a_plugin_appends_and_never_disables_someone_elses() {
        let source = lf(WITH_PLUGINS);
        let mut project = GodotProjectFile::parse(&source).expect("parses");
        assert!(project.enable_editor_plugin("res://addons/bhippi_studio/plugin.cfg"));
        // Idempotent, and a project-relative path names the same plugin as a res:// one.
        assert!(!project.enable_editor_plugin("res://addons/bhippi_studio/plugin.cfg"));
        assert!(!project.enable_editor_plugin("addons/bhippi_studio/plugin.cfg"));

        let text = project.to_text();
        assert!(text.contains(
            "enabled=PackedStringArray(\"res://addons/someone_elses_tool/plugin.cfg\", \"res://addons/bhippi_studio/plugin.cfg\")"
        ), "unexpected [editor_plugins]:\n{text}");
        assert_eq!(
            GodotProjectFile::parse(&text).expect("re-parses").to_text(),
            text
        );

        // Putting the key back the way the fixture had it restores it byte for byte, so the
        // append touched that one value and nothing else in the file.
        let mut restored = GodotProjectFile::parse(&text).expect("re-parses");
        restored.file.set(
            EDITOR_PLUGINS_SECTION,
            EDITOR_PLUGINS_KEY,
            TscnValue::Raw(packed_string_array(&[
                "res://addons/someone_elses_tool/plugin.cfg",
            ])),
        );
        assert_eq!(restored.to_text(), source);
    }

    #[test]
    fn a_project_without_an_editor_plugins_section_gains_one_in_alphabetical_order() {
        let source = lf(FIXTURE);
        let mut project = GodotProjectFile::parse(&source).expect("parses");
        assert!(project.editor_plugins().is_empty());
        assert!(project.enable_editor_plugin("addons/bhippi_studio/plugin.cfg"));

        let text = project.to_text();
        assert!(
            text.contains(
                "\n[editor_plugins]\n\nenabled=PackedStringArray(\"res://addons/bhippi_studio/plugin.cfg\")\n"
            ),
            "unexpected [editor_plugins]:\n{text}"
        );
        // Godot keeps its sections sorted; the new one lands between [display] and [input].
        let at = |name: &str| text.find(name).unwrap_or(usize::MAX);
        assert!(at("[display]") < at("[editor_plugins]"));
        assert!(at("[editor_plugins]") < at("[input]"));
        assert_eq!(
            GodotProjectFile::parse(&text).expect("re-parses").to_text(),
            text
        );

        // Nothing else moved: drop the added section and the fixture is back, byte for byte.
        let mut stripped = GodotProjectFile::parse(&text).expect("re-parses");
        assert!(stripped
            .file
            .remove(EDITOR_PLUGINS_SECTION, EDITOR_PLUGINS_KEY));
        assert_eq!(stripped.to_text(), source);
    }

    #[test]
    fn a_generated_project_file_has_the_banner_and_config_version() {
        let project = GodotProjectFile::new("Demo", "scenes/main.tscn", &["4.7", "Forward Plus"]);
        let text = project.to_text();
        assert!(text.starts_with("; Engine configuration file.\n"));
        assert!(text.contains("\nconfig_version=5\n\n[application]\n\n"));
        assert_eq!(
            GodotProjectFile::parse(&text).expect("parses").to_text(),
            text
        );
    }
}
