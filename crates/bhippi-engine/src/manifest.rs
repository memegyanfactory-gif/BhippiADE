use crate::error::{EngineError, Result};
use crate::GAME_MANIFEST_FILE;
use bhippi_types::GameId;
use serde::{Deserialize, Serialize};
use specta::Type;
use std::path::{Path, PathBuf};

/// A `.bhippi`-free, human-readable game manifest (`Bhippi.game.toml`, plan §8). Everything
/// here is plain text the user and the AI can both read and edit with normal file tools.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct GameManifest {
    pub game: GameSection,
    pub render: RenderSection,
    pub physics: PhysicsSection,
    pub targets: TargetSection,
    /// What the agent may do to this project (ENG-190). Absent means the shipped defaults,
    /// so every manifest written before this existed keeps working unchanged.
    #[serde(default)]
    pub agent: crate::capability::CapabilityPolicy,
    /// Which runtime builds this game. Absent means the pre-Godot Bhippi runtime, so every
    /// manifest written before the Godot runtime existed keeps parsing and keeps meaning
    /// exactly what it meant.
    #[serde(default)]
    pub runtime: crate::godot::manifest::GameRuntime,
    /// Godot-specific settings; `None` for a project that is not a Godot one.
    #[serde(default)]
    pub godot: Option<crate::godot::manifest::GodotManifestSection>,
    /// How this game is published (GAD-023/092). Absent means the shipped defaults, so
    /// every manifest written before publishing existed keeps working unchanged.
    #[serde(default)]
    pub publish: PublishSection,
}

/// The longest `[game] title`. A store listing, not a paragraph.
pub const MAX_GAME_TITLE_CHARS: usize = 80;
/// The longest `[game] description`.
pub const MAX_GAME_DESCRIPTION_CHARS: usize = 1_000;
/// The most `[game] tags` one game carries.
pub const MAX_GAME_TAGS: usize = 12;
/// The longest one tag may be.
pub const MAX_GAME_TAG_CHARS: usize = 32;

/// The `[publish]` table (GAD-023, GAD-092).
///
/// `deny_unknown_fields` on purpose, and only here: the rest of the manifest tolerates keys
/// it does not know so a newer Bhippi's file still opens in an older one, but `[publish]`
/// is a table people hand-edit and a mistyped `credit = false` that silently means "credits
/// on" is the kind of quiet failure ENG-190 exists to stop.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
#[serde(deny_unknown_fields)]
pub struct PublishSection {
    /// Write `credits.html` beside the export.
    #[serde(default = "default_credits")]
    pub credits: bool,
    /// Project-relative folder the web export lands in.
    #[serde(default = "default_web_export_dir")]
    pub web_export_dir: String,
}

fn default_credits() -> bool {
    true
}

fn default_web_export_dir() -> String {
    "export/web".to_owned()
}

impl Default for PublishSection {
    fn default() -> Self {
        Self {
            credits: default_credits(),
            web_export_dir: default_web_export_dir(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct GameSection {
    pub id: GameId,
    pub name: String,
    pub version: String,
    pub default_scene: String,
    pub engine_track: EngineTrack,
    /// Independent HUD scene. Play on Main/Level attaches this overlay.
    #[serde(default)]
    pub hud_scene: Option<String>,
    /// Ordered level scenes. Main Play starts at index 0.
    #[serde(default)]
    pub levels: Vec<String>,
    /// The player-facing title (GAD-023). Empty falls back to [`GameSection::name`], which
    /// is what every manifest written before this field existed says.
    #[serde(default)]
    pub title: String,
    /// One paragraph for the store listing and the credits page.
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub tags: Vec<String>,
    /// Project-relative poster image, e.g. `.bhippi/poster.png`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub poster: Option<String>,
}

impl GameSection {
    /// What to show a player: the title when one is set, else the project's name. The
    /// fallback is a rule, so it lives here and not in the webview (INV-073).
    #[must_use]
    pub fn display_title(&self) -> String {
        let title = self.title.trim();
        if title.is_empty() {
            self.name.trim().to_owned()
        } else {
            title.to_owned()
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum EngineTrack {
    Rust,
    Scripted,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct RenderSection {
    pub pipeline: RenderPipeline,
    pub msaa: u32,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum RenderPipeline {
    #[serde(alias = "3d")]
    D3d,
    #[serde(alias = "2d")]
    D2d,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct PhysicsSection {
    pub backend: PhysicsBackend,
    pub gravity: [f32; 3],
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum PhysicsBackend {
    Avian,
    None,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct TargetSection {
    pub windows: TargetConfig,
    pub android: AndroidConfig,
    pub ios: IosConfig,
    pub web: WebConfig,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct TargetConfig {
    pub enabled: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct AndroidConfig {
    pub enabled: bool,
    pub package: String,
    pub min_sdk: u32,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct IosConfig {
    pub enabled: bool,
    pub bundle_id: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct WebConfig {
    pub enabled: bool,
    pub canvas_fit: String,
}

impl GameManifest {
    /// The manifest's own defaults — the shape is stable even when sections are missing.
    #[must_use]
    pub fn defaults(project_name: &str) -> Self {
        Self {
            game: GameSection {
                id: GameId::new(),
                name: project_name.to_owned(),
                version: "0.1.0".to_owned(),
                default_scene: "assets/scenes/main.bscn.json".to_owned(),
                engine_track: EngineTrack::Rust,
                // Points at the HUD *document* (ENG-139), not the old widget-entity scene.
                hud_scene: None,
                levels: vec!["assets/scenes/level_01.bscn.json".to_owned()],
                title: String::new(),
                description: String::new(),
                tags: Vec::new(),
                poster: None,
            },
            render: RenderSection {
                pipeline: RenderPipeline::D3d,
                msaa: 4,
            },
            physics: PhysicsSection {
                backend: PhysicsBackend::Avian,
                gravity: [0.0, -9.81, 0.0],
            },
            targets: TargetSection {
                windows: TargetConfig { enabled: true },
                android: AndroidConfig {
                    enabled: false,
                    package: "com.example.mygame".to_owned(),
                    min_sdk: 24,
                },
                ios: IosConfig {
                    enabled: false,
                    bundle_id: "com.example.mygame".to_owned(),
                },
                web: WebConfig {
                    enabled: true,
                    canvas_fit: "window".to_owned(),
                },
            },
            agent: crate::capability::CapabilityPolicy::default(),
            runtime: crate::godot::manifest::GameRuntime::Bhippi,
            godot: None,
            publish: PublishSection::default(),
        }
    }

    /// Enabled desktop/mobile/web targets as stable slugs (used by the Build panel).
    #[must_use]
    pub fn enabled_targets(&self) -> Vec<&'static str> {
        let mut targets = Vec::new();
        if self.targets.windows.enabled {
            targets.push("windows");
        }
        if self.targets.android.enabled {
            targets.push("android");
        }
        if self.targets.ios.enabled {
            targets.push("ios");
        }
        if self.targets.web.enabled {
            targets.push("web");
        }
        // macOS/Linux are always available (host permits) — the manifest has no switch yet.
        targets.push("linux");
        targets.push("macos");
        targets.sort_unstable();
        targets
    }
}

/// Parse a `Bhippi.game.toml` document. Unknown fields are tolerated (forward
/// compatibility); the referenced sections must exist and validate.
pub fn parse_manifest(text: &str) -> Result<GameManifest> {
    let manifest: GameManifest = toml::from_str(text).map_err(|error| {
        EngineError::Manifest(
            format!("invalid YAML-free table layout: {error}"),
            Some("Fix the syntax or re-create the manifest with New Game Project.".to_owned()),
        )
    })?;
    validate_manifest(&manifest)?;
    Ok(manifest)
}

fn validate_manifest(manifest: &GameManifest) -> Result<()> {
    let name = manifest.game.name.trim();
    if name.is_empty() {
        return Err(EngineError::Manifest(
            "game.name must not be empty".to_owned(),
            Some("Give the game a name in Bhippi.game.toml.".to_owned()),
        ));
    }
    if manifest.render.msaa == 0 {
        return Err(EngineError::Manifest(
            "render.msaa must be > 0".to_owned(),
            Some("Choose 2, 4 or 8 in Bhippi.game.toml.".to_owned()),
        ));
    }
    // A Godot project's scenes are `.tscn` files under `scenes/`, so the Bhippi
    // scene-document rule cannot apply to them; `crate::godot::gates` checks the Godot
    // main scene instead, against `project.godot` and the file system.
    if manifest.runtime != crate::godot::manifest::GameRuntime::Godot
        && (!manifest.game.default_scene.starts_with("assets/scenes/")
            || !manifest.game.default_scene.ends_with(".bscn.json"))
    {
        return Err(EngineError::Manifest(
            "game.default_scene must point at an assets/scenes/*.bscn.json file".to_owned(),
            Some("Create a scene and set game.default_scene to its path.".to_owned()),
        ));
    }
    // A capability switch nobody validates is decorative: a typo in `[agent]` would read as
    // "use the default", which is the opposite of what the person typing it wanted.
    manifest.agent.validate()?;
    validate_game_settings(&manifest.game)?;
    validate_publish(&manifest.publish)?;
    Ok(())
}

/// The `[game]` presentation fields (GAD-023). Shared by the parser and by
/// `game_settings_set`, so a value the form refuses is a value the file refuses.
pub fn validate_game_settings(game: &GameSection) -> Result<()> {
    if game.display_title().is_empty() {
        return Err(EngineError::Manifest(
            "game.title must not be empty".to_owned(),
            Some("Give the game a title people will see.".to_owned()),
        ));
    }
    let title_length = game.title.trim().chars().count();
    if title_length > MAX_GAME_TITLE_CHARS {
        return Err(EngineError::Manifest(
            format!("game.title is {title_length} characters; the limit is {MAX_GAME_TITLE_CHARS}"),
            Some("A title is a name, not a tagline — the description holds the rest.".to_owned()),
        ));
    }
    let description_length = game.description.trim().chars().count();
    if description_length > MAX_GAME_DESCRIPTION_CHARS {
        return Err(EngineError::Manifest(
            format!(
                "game.description is {description_length} characters; the limit is \
                 {MAX_GAME_DESCRIPTION_CHARS}"
            ),
            Some("Trim it to a paragraph; the credits page renders it verbatim.".to_owned()),
        ));
    }
    if game.tags.len() > MAX_GAME_TAGS {
        return Err(EngineError::Manifest(
            format!(
                "game.tags has {} entries; the limit is {MAX_GAME_TAGS}",
                game.tags.len()
            ),
            Some("Keep the tags that would help somebody find the game.".to_owned()),
        ));
    }
    for tag in &game.tags {
        let trimmed = tag.trim();
        if trimmed.is_empty() {
            return Err(EngineError::Manifest(
                "game.tags contains an empty tag".to_owned(),
                Some("Remove the blank entry.".to_owned()),
            ));
        }
        if trimmed.chars().count() > MAX_GAME_TAG_CHARS {
            return Err(EngineError::Manifest(
                format!("the tag `{trimmed}` is longer than {MAX_GAME_TAG_CHARS} characters"),
                Some("A tag is one word or two.".to_owned()),
            ));
        }
    }
    if let Some(poster) = &game.poster {
        check_inside_project("game.poster", poster)?;
    }
    Ok(())
}

/// The `[publish]` table's values. Unknown *keys* are refused by serde; this is the rest.
pub fn validate_publish(publish: &PublishSection) -> Result<()> {
    check_inside_project("publish.web_export_dir", &publish.web_export_dir)
}

/// A project-relative path that cannot climb out of the project or name a drive.
///
/// The rule is lexical on purpose: it runs on a manifest that may name a file which does
/// not exist yet, so there is nothing to canonicalise, and a check that only worked for
/// existing files would pass the manifest and fail the export.
fn check_inside_project(field: &str, value: &str) -> Result<()> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(EngineError::Manifest(
            format!("{field} must not be empty"),
            Some("Give a path relative to the project folder.".to_owned()),
        ));
    }
    let normalised = trimmed.replace('\\', "/");
    let escapes = normalised.starts_with('/')
        || normalised.contains(':')
        || normalised
            .split('/')
            .any(|segment| segment == ".." || segment.trim() != segment);
    if escapes {
        return Err(EngineError::Manifest(
            format!("{field} must stay inside the project: `{trimmed}` does not"),
            Some(
                "Use a path like `.bhippi/poster.png`, relative to the project folder.".to_owned(),
            ),
        ));
    }
    Ok(())
}

/// `true` when `text` begins like a game manifest (cheap pre-check for the pill).
#[must_use]
pub fn is_manifest_text(text: &str) -> bool {
    let trimmed = text.trim_start();
    trimmed.starts_with("[game]") && trimmed.contains("default_scene")
}

/// The manifest path for a project root.
#[must_use]
pub fn manifest_path(project_root: &Path) -> PathBuf {
    project_root.join(GAME_MANIFEST_FILE)
}

/// Loads and parses `Bhippi.game.toml` from a project root. `Ok(None)` means the project
/// has no game manifest (the Engine pane shows the empty/create state).
pub fn load_manifest(project_root: &Path) -> Result<Option<GameManifest>> {
    let path = manifest_path(project_root);
    if !path.is_file() {
        return Ok(None);
    }
    let text = std::fs::read_to_string(&path).map_err(|error| EngineError::Io {
        operation: "read",
        path: path.display().to_string(),
        reason: error.to_string(),
        hint: Some("Make sure Bhippi.game.toml is readable.".to_owned()),
    })?;
    parse_manifest(&text).map(Some)
}

#[cfg(test)]
mod tests {
    use super::{parse_manifest, EngineTrack, GameManifest, RenderPipeline};

    const SAMPLE: &str = r#"
[game]
id = "01JC7B0KZ0TCVZVWY5YE2H3ZZQ"
name = "My Game"
version = "0.1.0"
default_scene = "assets/scenes/level_01.bscn.json"
engine_track = "rust"

[render]
pipeline = "3d"
msaa = 4

[physics]
backend = "avian"
gravity = [0.0, -9.81, 0.0]

[targets.windows]
enabled = true
[targets.android]
enabled = false
package = "com.example.mygame"
min_sdk = 24
[targets.ios]
enabled = false
bundle_id = "com.example.mygame"
[targets.web]
enabled = true
canvas_fit = "window"
"#;

    #[test]
    fn manifest_round_trips_and_enabled_targets_are_sorted() {
        let manifest = parse_manifest(SAMPLE).map_err(|e| e.to_string()).ok();
        assert!(manifest.is_some());
        let manifest = manifest.unwrap_or_else(|| GameManifest::defaults("x"));
        assert_eq!(manifest.game.engine_track, EngineTrack::Rust);
        assert_eq!(manifest.render.pipeline, RenderPipeline::D3d);
        let targets = manifest.enabled_targets();
        assert_eq!(targets, vec!["linux", "macos", "web", "windows"]);
    }

    #[test]
    fn defaults_are_valid_and_deterministic() {
        let defaults = GameManifest::defaults("Demo");
        let rendered = toml::to_string(&defaults).expect("manifest must serialize");

        assert_eq!(toml::to_string(&defaults).expect("serialize"), rendered);
        assert!(parse_manifest(&rendered).is_ok());
    }

    #[test]
    fn bad_default_scene_is_rejected() {
        let mut manifest = GameManifest::defaults("Demo");
        manifest.game.default_scene = "level_01.bscn.json".to_owned();
        let text = toml::to_string(&manifest).expect("serialize");

        let error = parse_manifest(&text).expect_err("must reject");
        assert!(error.hint().is_some());
    }
}
