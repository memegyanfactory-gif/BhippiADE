//! `export_presets.cfg` — what `--export-release` / `--export-debug` read.
//!
//! Godot's CLI export takes a **preset name**, and a preset only exists in this file. A
//! project without it cannot be exported from the command line at all, which is why the
//! scaffold writes one and the gates check for it.
//!
//! The file is the same INI grammar as `project.godot`, so [`super::project::GodotIniFile`]
//! parses it and a preset is two sections: `[preset.N]` with the metadata and
//! `[preset.N.options]` with the platform switches. Adding or replacing a preset by name
//! leaves every other preset — including ones the editor wrote — untouched, and re-numbers
//! the sections so the indices stay contiguous, which is what Godot expects.

use super::project::{parse_ini, GodotIniFile, IniSection};
use super::tscn::TscnValue;
use crate::error::Result;
use serde::{Deserialize, Serialize};
use specta::Type;

/// The preset name Bhippi gives the web build.
pub const WEB_PRESET_NAME: &str = "Web";
/// The preset name Bhippi gives the Windows build.
pub const WINDOWS_PRESET_NAME: &str = "Windows Desktop";
/// Godot's platform id for the web target.
pub const WEB_PLATFORM: &str = "Web";
/// Godot's platform id for the Windows target.
pub const WINDOWS_PLATFORM: &str = "Windows Desktop";
/// Where a web export lands, relative to the project root.
pub const WEB_EXPORT_PATH: &str = "export/web/index.html";
/// The directory Windows exports land in, relative to the project root.
pub const WINDOWS_EXPORT_DIR: &str = "export/windows";
/// `html/canvas_resize_policy`: 2 is "adjust the canvas to the whole window", the only
/// value that makes an embedded export fill its iframe.
pub const CANVAS_RESIZE_POLICY_WINDOW: i64 = 2;

/// Which target a generated preset is for.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum PresetTarget {
    Web,
    Windows,
}

impl PresetTarget {
    #[must_use]
    pub fn preset_name(self) -> &'static str {
        match self {
            Self::Web => WEB_PRESET_NAME,
            Self::Windows => WINDOWS_PRESET_NAME,
        }
    }

    #[must_use]
    pub fn platform(self) -> &'static str {
        match self {
            Self::Web => WEB_PLATFORM,
            Self::Windows => WINDOWS_PLATFORM,
        }
    }
}

/// One preset as a pair of sections, still un-numbered.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct ExportPreset {
    pub name: String,
    pub entries: Vec<(String, TscnValue)>,
    pub options: Vec<(String, TscnValue)>,
}

/// A parsed `export_presets.cfg`.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize, Type)]
pub struct ExportPresets {
    pub presets: Vec<ExportPreset>,
}

impl ExportPresets {
    pub fn parse(text: &str) -> Result<Self> {
        let file = parse_ini(text)?;
        let mut presets: Vec<ExportPreset> = Vec::new();
        for section in &file.sections {
            let Some(rest) = section.name.strip_prefix("preset.") else {
                continue;
            };
            if let Some(index) = rest.strip_suffix(".options") {
                let slot = preset_slot(index, &presets);
                let Some(preset) = presets.get_mut(slot) else {
                    continue;
                };
                preset.options = section.entries.clone();
                continue;
            }
            presets.push(ExportPreset {
                name: section
                    .get("name")
                    .and_then(|value| value.as_str().map(str::to_owned))
                    .unwrap_or_default(),
                entries: section.entries.clone(),
                options: Vec::new(),
            });
        }
        Ok(Self { presets })
    }

    #[must_use]
    pub fn preset(&self, name: &str) -> Option<&ExportPreset> {
        self.presets.iter().find(|preset| preset.name == name)
    }

    #[must_use]
    pub fn has_preset(&self, name: &str) -> bool {
        self.preset(name).is_some()
    }

    #[must_use]
    pub fn names(&self) -> Vec<String> {
        self.presets
            .iter()
            .map(|preset| preset.name.clone())
            .collect()
    }

    /// Add a preset, or replace the one with the same name **in place** so an edit does not
    /// reorder a file the editor also writes.
    pub fn upsert(&mut self, preset: ExportPreset) {
        match self
            .presets
            .iter()
            .position(|existing| existing.name == preset.name)
        {
            Some(at) => {
                if let Some(slot) = self.presets.get_mut(at) {
                    *slot = preset;
                }
            }
            None => self.presets.push(preset),
        }
    }

    pub fn remove(&mut self, name: &str) -> bool {
        let before = self.presets.len();
        self.presets.retain(|preset| preset.name != name);
        self.presets.len() != before
    }

    #[must_use]
    pub fn to_text(&self) -> String {
        let mut file = GodotIniFile::default();
        for (index, preset) in self.presets.iter().enumerate() {
            file.sections.push(IniSection {
                name: format!("preset.{index}"),
                entries: preset.entries.clone(),
            });
            file.sections.push(IniSection {
                name: format!("preset.{index}.options"),
                entries: preset.options.clone(),
            });
        }
        file.to_text()
    }
}

fn preset_slot(index: &str, presets: &[ExportPreset]) -> usize {
    index
        .parse::<usize>()
        .ok()
        .filter(|slot| *slot < presets.len())
        .unwrap_or(presets.len().saturating_sub(1))
}

fn entry(key: &str, value: TscnValue) -> (String, TscnValue) {
    (key.to_owned(), value)
}

/// The metadata section every preset carries, whatever its platform.
fn common_entries(name: &str, platform: &str, export_path: &str) -> Vec<(String, TscnValue)> {
    vec![
        entry("name", TscnValue::str(name)),
        entry("platform", TscnValue::str(platform)),
        entry("runnable", TscnValue::Bool(true)),
        entry("advanced_options", TscnValue::Bool(false)),
        entry("dedicated_server", TscnValue::Bool(false)),
        entry("custom_features", TscnValue::str("")),
        entry("export_filter", TscnValue::str("all_resources")),
        entry("include_filter", TscnValue::str("")),
        entry("exclude_filter", TscnValue::str("")),
        entry("export_path", TscnValue::str(export_path)),
        entry("patches", TscnValue::Raw("PackedStringArray()".to_owned())),
        entry("encryption_include_filters", TscnValue::str("")),
        entry("encryption_exclude_filters", TscnValue::str("")),
        entry("seed", TscnValue::Int(0)),
        entry("encrypt_pck", TscnValue::Bool(false)),
        entry("encrypt_directory", TscnValue::Bool(false)),
        entry("script_export_mode", TscnValue::Int(2)),
    ]
}

/// The Web preset.
///
/// `variant/thread_support=false` is deliberate: the threaded web template needs
/// cross-origin isolation headers (`COOP`/`COEP`) that a plain `<iframe>` preview does not
/// have, and the export silently fails to start there. The single-threaded template runs
/// anywhere, which is what a preview is for.
#[must_use]
pub fn web_preset() -> ExportPreset {
    ExportPreset {
        name: WEB_PRESET_NAME.to_owned(),
        entries: common_entries(WEB_PRESET_NAME, WEB_PLATFORM, WEB_EXPORT_PATH),
        options: vec![
            entry("custom_template/debug", TscnValue::str("")),
            entry("custom_template/release", TscnValue::str("")),
            entry("variant/extensions_support", TscnValue::Bool(false)),
            entry("variant/thread_support", TscnValue::Bool(false)),
            entry(
                "vram_texture_compression/for_desktop",
                TscnValue::Bool(true),
            ),
            entry(
                "vram_texture_compression/for_mobile",
                TscnValue::Bool(false),
            ),
            entry("html/export_icon", TscnValue::Bool(true)),
            entry("html/custom_html_shell", TscnValue::str("")),
            entry("html/head_include", TscnValue::str("")),
            entry(
                "html/canvas_resize_policy",
                TscnValue::Int(CANVAS_RESIZE_POLICY_WINDOW),
            ),
            entry("html/focus_canvas_on_start", TscnValue::Bool(true)),
            entry("html/experimental_virtual_keyboard", TscnValue::Bool(false)),
            entry("progressive_web_app/enabled", TscnValue::Bool(false)),
        ],
    }
}

/// The Windows Desktop preset. `game_name` becomes the executable's file stem.
#[must_use]
pub fn windows_preset(game_name: &str) -> ExportPreset {
    let stem = executable_stem(game_name);
    ExportPreset {
        name: WINDOWS_PRESET_NAME.to_owned(),
        entries: common_entries(
            WINDOWS_PRESET_NAME,
            WINDOWS_PLATFORM,
            &format!("{WINDOWS_EXPORT_DIR}/{stem}.exe"),
        ),
        options: vec![
            entry("custom_template/debug", TscnValue::str("")),
            entry("custom_template/release", TscnValue::str("")),
            entry("debug/export_console_wrapper", TscnValue::Int(1)),
            entry("binary_format/embed_pck", TscnValue::Bool(false)),
            entry("texture_format/s3tc_bptc", TscnValue::Bool(true)),
            entry("texture_format/etc2_astc", TscnValue::Bool(false)),
            entry("binary_format/architecture", TscnValue::str("x86_64")),
            entry("codesign/enable", TscnValue::Bool(false)),
            entry("application/modify_resources", TscnValue::Bool(true)),
            entry("application/icon", TscnValue::str("")),
            entry("application/console_wrapper_icon", TscnValue::str("")),
            entry("application/file_version", TscnValue::str("")),
            entry("application/product_version", TscnValue::str("")),
            entry("application/company_name", TscnValue::str("")),
            entry("application/product_name", TscnValue::str(game_name)),
            entry("application/file_description", TscnValue::str("")),
            entry("application/copyright", TscnValue::str("")),
            entry("application/trademarks", TscnValue::str("")),
            entry("application/export_angle", TscnValue::Int(0)),
            entry("application/export_d3d12", TscnValue::Int(0)),
            entry("ssh_remote_deploy/enabled", TscnValue::Bool(false)),
        ],
    }
}

/// A file-system-safe stem for the exported executable.
#[must_use]
pub fn executable_stem(game_name: &str) -> String {
    let stem: String = game_name
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || character == '-' || character == '_' {
                character
            } else {
                '_'
            }
        })
        .collect();
    let trimmed = stem.trim_matches('_');
    if trimmed.is_empty() {
        "game".to_owned()
    } else {
        trimmed.to_owned()
    }
}

/// The `export_presets.cfg` a new project ships with: Web plus Windows Desktop.
#[must_use]
pub fn default_presets(game_name: &str) -> ExportPresets {
    ExportPresets {
        presets: vec![web_preset(), windows_preset(game_name)],
    }
}

#[cfg(test)]
mod tests {
    use super::{
        default_presets, executable_stem, web_preset, windows_preset, ExportPresets,
        WEB_PRESET_NAME, WINDOWS_PRESET_NAME,
    };

    const FIXTURE: &str = include_str!("../../../../tests/fixtures/godot/export_presets.cfg");

    fn lf(text: &str) -> String {
        text.replace("\r\n", "\n")
    }

    #[test]
    fn a_real_export_file_round_trips_and_keeps_its_presets() {
        let source = lf(FIXTURE);
        let presets = ExportPresets::parse(&source).expect("parses");
        assert_eq!(presets.names(), vec!["Web", "Linux/X11"]);
        assert_eq!(presets.to_text(), source);
    }

    #[test]
    fn replacing_one_preset_leaves_the_others_exactly_as_they_were() {
        let mut presets = ExportPresets::parse(&lf(FIXTURE)).expect("parses");
        let linux_before = presets
            .preset("Linux/X11")
            .cloned()
            .expect("the fixture has a Linux preset");

        presets.upsert(web_preset());
        assert_eq!(presets.names(), vec!["Web", "Linux/X11"], "order is kept");
        assert_eq!(presets.preset("Linux/X11"), Some(&linux_before));

        let text = presets.to_text();
        assert!(text.contains("[preset.0]"));
        assert!(text.contains("[preset.1.options]"));
        assert_eq!(
            ExportPresets::parse(&text).expect("re-parses").to_text(),
            text
        );
    }

    #[test]
    fn the_web_preset_uses_the_template_a_plain_iframe_can_run() {
        let preset = web_preset();
        assert_eq!(preset.name, WEB_PRESET_NAME);
        let option = |key: &str| {
            preset
                .options
                .iter()
                .find(|(name, _)| name == key)
                .map(|(_, value)| value.to_text())
        };
        assert_eq!(option("variant/thread_support").as_deref(), Some("false"));
        assert_eq!(option("html/canvas_resize_policy").as_deref(), Some("2"));
        let entry = |key: &str| {
            preset
                .entries
                .iter()
                .find(|(name, _)| name == key)
                .map(|(_, value)| value.to_text())
        };
        assert_eq!(entry("platform").as_deref(), Some("\"Web\""));
        assert_eq!(
            entry("export_path").as_deref(),
            Some("\"export/web/index.html\"")
        );
    }

    #[test]
    fn the_windows_preset_names_the_executable_after_the_game() {
        let preset = windows_preset("My Game!");
        assert_eq!(preset.name, WINDOWS_PRESET_NAME);
        assert!(preset
            .entries
            .iter()
            .any(|(key, value)| key == "export_path"
                && value.to_text() == "\"export/windows/My_Game.exe\""));
        assert_eq!(executable_stem("  "), "game");
        assert_eq!(executable_stem("Level-1_x"), "Level-1_x");
    }

    #[test]
    fn a_generated_file_parses_back_into_the_same_presets() {
        let presets = default_presets("Demo");
        let text = presets.to_text();
        assert!(text.starts_with("[preset.0]\n\n"));
        let reparsed = ExportPresets::parse(&text).expect("parses");
        assert_eq!(reparsed, presets);
        assert!(reparsed.has_preset("Web"));
        assert!(reparsed.has_preset("Windows Desktop"));
    }
}
