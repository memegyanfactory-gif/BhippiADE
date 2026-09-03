//! The `[godot]` half of `Bhippi.game.toml`.
//!
//! `Bhippi.game.toml` is still the project marker and still the file a human edits; a Godot
//! project adds `runtime = "godot"` and a `[godot]` table to it. Both are `#[serde(default)]`
//! on [`GameManifest`](crate::manifest::GameManifest), so every manifest written before this
//! existed keeps parsing and keeps meaning what it meant.

use super::detect::GODOT_PINNED_VERSION;
use super::probe::PROBE_AUTOLOAD_NAME;
use crate::capability::CapabilityPolicy;
use crate::manifest::EngineTrack;
use crate::manifest::{
    AndroidConfig, GameManifest, GameSection, IosConfig, PhysicsBackend, PhysicsSection,
    RenderPipeline, RenderSection, TargetConfig, TargetSection, WebConfig,
};
use serde::{Deserialize, Serialize};
use specta::Type;

/// The default main scene a scaffolded Godot project points at.
pub const DEFAULT_MAIN_SCENE: &str = "scenes/main.tscn";

/// Which runtime builds this project.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum GameRuntime {
    /// The pre-Godot Bhippi runtime. The default, so an existing manifest is unchanged.
    #[default]
    Bhippi,
    Godot,
}

impl GameRuntime {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Bhippi => "bhippi",
            Self::Godot => "godot",
        }
    }
}

/// The `[godot]` table.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct GodotManifestSection {
    /// The Godot version the project was authored against.
    pub version_pin: String,
    /// The project-relative main scene, mirroring `project.godot`'s `run/main_scene`.
    pub main_scene: String,
    /// Whether the Bhippi probe autoload is registered.
    #[serde(default = "default_true")]
    pub probe: bool,
}

fn default_true() -> bool {
    true
}

impl Default for GodotManifestSection {
    fn default() -> Self {
        Self {
            version_pin: GODOT_PINNED_VERSION.to_owned(),
            main_scene: DEFAULT_MAIN_SCENE.to_owned(),
            probe: true,
        }
    }
}

/// The manifest a new Godot project ships with.
#[must_use]
pub fn godot_manifest(name: &str, main_scene: &str, pipeline: RenderPipeline) -> GameManifest {
    let mut manifest = GameManifest {
        game: GameSection {
            id: bhippi_types::GameId::new(),
            name: name.to_owned(),
            version: "0.1.0".to_owned(),
            // A Godot project's scenes are `.tscn` files; `default_scene` names the same one
            // `project.godot` and `[godot].main_scene` do, so the three cannot drift.
            default_scene: main_scene.to_owned(),
            engine_track: EngineTrack::Scripted,
            hud_scene: None,
            levels: Vec::new(),
            title: String::new(),
            description: String::new(),
            tags: Vec::new(),
            poster: None,
        },
        render: RenderSection { pipeline, msaa: 2 },
        physics: PhysicsSection {
            backend: PhysicsBackend::None,
            gravity: [0.0, -9.8, 0.0],
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
        agent: CapabilityPolicy::default(),
        runtime: GameRuntime::Godot,
        publish: crate::manifest::PublishSection::default(),
        godot: Some(GodotManifestSection {
            version_pin: GODOT_PINNED_VERSION.to_owned(),
            main_scene: main_scene.to_owned(),
            probe: true,
        }),
    };
    manifest.game.name = name.trim().to_owned();
    manifest
}

/// Render a Godot project's `Bhippi.game.toml`.
///
/// Written by hand rather than through `toml::to_string` for the same reason
/// [`crate::scaffold::format_manifest`] is: the top-level `runtime` key has to come before
/// the first `[table]` header or TOML reads it as part of that table, and the section order
/// is what makes the file readable to the person who opens it.
#[must_use]
pub fn render_manifest(manifest: &GameManifest) -> String {
    let godot = manifest.godot.clone().unwrap_or_default();
    let pipeline = match manifest.render.pipeline {
        RenderPipeline::D3d => "3d",
        RenderPipeline::D2d => "2d",
    };
    let track = match manifest.game.engine_track {
        EngineTrack::Rust => "rust",
        EngineTrack::Scripted => "scripted",
    };
    let backend = match manifest.physics.backend {
        PhysicsBackend::Avian => "avian",
        PhysicsBackend::None => "none",
    };
    format!(
        "# {name} — Bhippi game project (format v1, Godot runtime)\n\
         # Generated by Bhippi. Edit freely; sections are validated on load.\n\
         # `runtime` must stay above the first [table] or TOML reads it as part of it.\n\n\
         runtime = \"{runtime}\"\n\n\
         [game]\n\
         id = \"{id}\"\n\
         name = \"{name}\"\n\
         version = \"{version}\"\n\
         default_scene = \"{default_scene}\"\n\
         engine_track = \"{track}\"\n\
         # What a player sees (GAD-023). An empty title falls back to `name`.\n\
         title = \"{title}\"\n\
         description = \"{description}\"\n\
         tags = [{tags}]\n\
         {poster}\n\
         [godot]\n\
         # The Godot the project was authored against. Bhippi runs {minimum}+ and pins this.\n\
         version_pin = \"{version_pin}\"\n\
         main_scene = \"{main_scene}\"\n\
         # The {probe_name} autoload: scripted input and playtest telemetry.\n\
         probe = {probe}\n\n\
         [render]\n\
         pipeline = \"{pipeline}\"\n\
         msaa = {msaa}\n\n\
         [physics]\n\
         # Godot owns physics; these values are what the editor reports, not a second engine.\n\
         backend = \"{backend}\"\n\
         gravity = {gravity:?}\n\n\
         [targets.windows]\n\
         enabled = {windows}\n\
         [targets.android]\n\
         enabled = {android}\n\
         package = \"{package}\"\n\
         min_sdk = {min_sdk}\n\
         [targets.ios]\n\
         enabled = {ios}\n\
         bundle_id = \"{bundle_id}\"\n\
         [targets.web]\n\
         enabled = {web}\n\
         canvas_fit = \"{canvas_fit}\"\n\n\
         # How this game is published (GAD-092). A key that is not one of these two is\n\
         # refused on load rather than ignored.\n\
         [publish]\n\
         credits = {credits}\n\
         web_export_dir = \"{web_export_dir}\"\n\n\
         # What the agent may do to this project (ENG-190). Each key is allow / ask / deny;\n\
         # anything not listed takes its default.\n\
         [agent]\n\
         {agent}",
        runtime = manifest.runtime.as_str(),
        id = manifest.game.id,
        name = manifest.game.name,
        version = manifest.game.version,
        default_scene = manifest.game.default_scene,
        track = track,
        title = escape_toml(&manifest.game.title),
        description = escape_toml(&manifest.game.description),
        tags = manifest
            .game
            .tags
            .iter()
            .map(|tag| format!("\"{}\"", escape_toml(tag)))
            .collect::<Vec<_>>()
            .join(", "),
        // Written only when there is one: `poster = ""` would parse as a poster at the
        // project root, which is a different claim from "this game has no poster".
        poster = match &manifest.game.poster {
            Some(poster) => format!("poster = \"{}\"\n", escape_toml(poster)),
            None => String::new(),
        },
        credits = manifest.publish.credits,
        web_export_dir = escape_toml(&manifest.publish.web_export_dir),
        version_pin = godot.version_pin,
        main_scene = godot.main_scene,
        probe = godot.probe,
        probe_name = PROBE_AUTOLOAD_NAME,
        minimum = {
            let (major, minor) = super::detect::GODOT_MINIMUM;
            format!("{major}.{minor}")
        },
        pipeline = pipeline,
        msaa = manifest.render.msaa,
        backend = backend,
        gravity = manifest.physics.gravity,
        windows = manifest.targets.windows.enabled,
        android = manifest.targets.android.enabled,
        package = manifest.targets.android.package,
        min_sdk = manifest.targets.android.min_sdk,
        ios = manifest.targets.ios.enabled,
        bundle_id = manifest.targets.ios.bundle_id,
        web = manifest.targets.web.enabled,
        canvas_fit = manifest.targets.web.canvas_fit,
        agent = toml::to_string(&manifest.agent).unwrap_or_default(),
    )
}

/// Make a string safe inside a TOML basic string.
///
/// The presentation fields are typed by a person, so a quote or a backslash in a title has
/// to survive the round trip rather than producing a file that no longer parses. Control
/// characters are dropped: TOML forbids them in a basic string and no title needs one.
fn escape_toml(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for character in text.chars() {
        match character {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => {}
            other if other.is_control() => {}
            other => out.push(other),
        }
    }
    out
}

/// True when this manifest describes a Godot project.
#[must_use]
pub fn is_godot(manifest: &GameManifest) -> bool {
    manifest.runtime == GameRuntime::Godot
}

#[cfg(test)]
mod tests {
    use super::{godot_manifest, is_godot, render_manifest, GameRuntime, DEFAULT_MAIN_SCENE};
    use crate::manifest::{parse_manifest, GameManifest, RenderPipeline};

    #[test]
    fn a_godot_manifest_round_trips_through_the_shared_parser() {
        let manifest = godot_manifest("My Game", DEFAULT_MAIN_SCENE, RenderPipeline::D3d);
        let text = render_manifest(&manifest);
        let parsed = parse_manifest(&text).expect("the Godot manifest must parse");

        assert!(is_godot(&parsed));
        assert_eq!(parsed.runtime, GameRuntime::Godot);
        let godot = parsed.godot.clone().expect("a [godot] table");
        assert_eq!(godot.main_scene, DEFAULT_MAIN_SCENE);
        assert_eq!(godot.version_pin, "4.7.1");
        assert!(godot.probe);
        assert_eq!(parsed.game.name, "My Game");
        assert_eq!(parsed.game.default_scene, DEFAULT_MAIN_SCENE);
    }

    /// The whole point of the `#[serde(default)]` on both new fields: a manifest written
    /// before the Godot runtime existed still parses, and still means "not Godot".
    #[test]
    fn a_pre_godot_manifest_still_parses_and_is_not_godot() {
        let legacy = crate::scaffold::format_manifest(&GameManifest::defaults("Legacy"));
        let parsed = parse_manifest(&legacy).expect("the old writer's output still parses");
        assert_eq!(parsed.runtime, GameRuntime::Bhippi);
        assert!(parsed.godot.is_none());
        assert!(!is_godot(&parsed));
    }

    /// GAD-023: the presentation fields and `[publish]` survive a write-read-write cycle,
    /// including a title with the characters that would otherwise end the string.
    #[test]
    fn the_game_settings_and_publish_tables_round_trip() {
        let mut manifest = godot_manifest("Demo", DEFAULT_MAIN_SCENE, RenderPipeline::D3d);
        manifest.game.title = "Feather \"Quest\" \\ 2".to_owned();
        manifest.game.description = "Collect ten feathers.\nThen fly.".to_owned();
        manifest.game.tags = vec!["cozy".to_owned(), "exploration".to_owned()];
        manifest.game.poster = Some(".bhippi/poster.png".to_owned());
        manifest.publish.credits = false;
        manifest.publish.web_export_dir = "export/site".to_owned();

        let text = render_manifest(&manifest);
        let parsed = parse_manifest(&text).expect("the writer's own output parses");
        assert_eq!(parsed.game.title, manifest.game.title);
        assert_eq!(parsed.game.description, manifest.game.description);
        assert_eq!(parsed.game.tags, vec!["cozy", "exploration"]);
        assert_eq!(parsed.game.poster.as_deref(), Some(".bhippi/poster.png"));
        assert!(!parsed.publish.credits);
        assert_eq!(parsed.publish.web_export_dir, "export/site");
        assert_eq!(parsed.game.display_title(), manifest.game.title);

        // Re-rendering the parse is byte-identical, so a settings save is idempotent.
        assert_eq!(render_manifest(&parsed), text);
    }

    /// A game with no poster writes no `poster` key at all — `poster = ""` would be a claim
    /// about a file at the project root rather than the absence of one.
    #[test]
    fn no_poster_writes_no_poster_key_and_the_title_falls_back_to_the_name() {
        let manifest = godot_manifest("Bare Game", DEFAULT_MAIN_SCENE, RenderPipeline::D3d);
        let text = render_manifest(&manifest);
        assert!(!text.contains("poster ="));
        let parsed = parse_manifest(&text).expect("parses");
        assert!(parsed.game.poster.is_none());
        assert_eq!(parsed.game.display_title(), "Bare Game");
        assert!(parsed.publish.credits, "credits default to on");
        assert_eq!(parsed.publish.web_export_dir, "export/web");
    }

    /// ENG-190's rule, applied to `[publish]`: a mistyped key is refused with a hint, not
    /// silently ignored into meaning its default.
    #[test]
    fn a_typo_in_publish_is_refused_and_a_bad_value_names_its_field() {
        let mut text = render_manifest(&godot_manifest(
            "Demo",
            DEFAULT_MAIN_SCENE,
            RenderPipeline::D3d,
        ));
        text = text.replace("credits = true", "credit = true");
        let error = parse_manifest(&text).expect_err("a typo must block");
        assert!(error.to_string().contains("credit"), "{error}");
        assert!(error.hint().is_some());

        let escaping = render_manifest(&godot_manifest(
            "Demo",
            DEFAULT_MAIN_SCENE,
            RenderPipeline::D3d,
        ))
        .replace(
            "web_export_dir = \"export/web\"",
            "web_export_dir = \"../elsewhere\"",
        );
        let error = parse_manifest(&escaping).expect_err("a path out of the project must block");
        assert!(
            error.to_string().contains("publish.web_export_dir"),
            "{error}"
        );
    }

    /// `runtime` sits above `[game]` on purpose — below it, TOML would read it as
    /// `game.runtime` and the project would silently look like a Bhippi one.
    #[test]
    fn the_runtime_key_is_written_above_the_first_table() {
        let text = render_manifest(&godot_manifest(
            "Demo",
            DEFAULT_MAIN_SCENE,
            RenderPipeline::D2d,
        ));
        let runtime_at = text.find("runtime = \"godot\"").expect("runtime key");
        let first_table = text.find("\n[").expect("a table header");
        assert!(runtime_at < first_table);
        assert!(text.contains("pipeline = \"2d\""));
    }
}
