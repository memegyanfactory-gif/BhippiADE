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
                hud_scene: Some("assets/ui/hud_main.hud.json".to_owned()),
                levels: vec!["assets/scenes/level_01.bscn.json".to_owned()],
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
    if !manifest.game.default_scene.starts_with("assets/scenes/")
        || !manifest.game.default_scene.ends_with(".bscn.json")
    {
        return Err(EngineError::Manifest(
            "game.default_scene must point at an assets/scenes/*.bscn.json file".to_owned(),
            Some("Create a scene and set game.default_scene to its path.".to_owned()),
        ));
    }
    // A capability switch nobody validates is decorative: a typo in `[agent]` would read as
    // "use the default", which is the opposite of what the person typing it wanted.
    manifest.agent.validate()?;
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
