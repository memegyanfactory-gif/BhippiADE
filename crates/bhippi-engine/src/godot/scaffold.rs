//! A new Godot project, written from the models rather than from string templates.
//!
//! Every file here comes out of [`TscnDocument`], [`GodotProjectFile`] or [`ExportPresets`],
//! so the scaffold's own output is exactly what the parsers accept — a template that drifts
//! from the parser is a "new project" that Bhippi itself cannot open, and that is a bug you
//! only find in front of a user.
//!
//! The GDScript is real Godot 4: `CharacterBody3D` with gravity read from
//! `ProjectSettings`, `move_and_slide`, and the `jump` action the project also defines.
//! `tests/godot_live.rs` runs `--check-only` over it against a real Godot.

use super::export_presets::default_presets;
use super::manifest::{godot_manifest, render_manifest, DEFAULT_MAIN_SCENE};
use super::probe::{probe_source, PROBE_AUTOLOAD_NAME, PROBE_REL_PATH, PROBE_RES_PATH};
use super::project::{GodotIniFile, GodotProjectFile};
use super::tscn::{SubResource, TscnDocument, TscnNode, TscnValue};
use crate::error::{EngineError, Result};
use crate::manifest::RenderPipeline;
use serde::{Deserialize, Serialize};
use specta::Type;
use std::path::{Path, PathBuf};

/// The scene every template writes.
pub const MAIN_SCENE_REL: &str = "scenes/main.tscn";
/// The project icon.
pub const ICON_REL: &str = "icon.svg";
/// The export presets file.
pub const EXPORT_PRESETS_REL: &str = "export_presets.cfg";
/// `project.godot`.
pub const PROJECT_REL: &str = "project.godot";
/// The movement actions every template defines.
pub const MOVE_ACTIONS: &[&str] = &["move_left", "move_right", "move_forward", "move_back"];
/// The jump action the 3D templates use.
pub const JUMP_ACTION: &str = "jump";
/// The viewport a new project opens at.
pub const VIEWPORT_WIDTH: i64 = 1280;
/// The viewport a new project opens at.
pub const VIEWPORT_HEIGHT: i64 = 720;
/// The `config/features` version tag. Must track [`super::detect::GODOT_PINNED_VERSION`].
pub const FEATURE_VERSION: &str = "4.7";

/// The addon descriptor Godot reads, project-relative.
pub const STUDIO_ADDON_CFG_REL: &str = "addons/bhippi_studio/plugin.cfg";
/// The addon script, project-relative.
pub const STUDIO_ADDON_SCRIPT_REL: &str = "addons/bhippi_studio/plugin.gd";
/// The same descriptor as the `res://` path `[editor_plugins] enabled` lists.
pub const STUDIO_ADDON_RES_PATH: &str = "res://addons/bhippi_studio/plugin.cfg";
/// The addon's display name in Project Settings → Plugins.
pub const STUDIO_ADDON_NAME: &str = "Bhippi Studio";
/// The addon version. Cosmetic to Godot; bump it when the files below change so a project
/// carrying an older copy is visibly older in the editor's plugin list. `1.1` added the live
/// follower (GAD-170): the editor rescans, opens the scene Bhippi just touched and selects
/// what it wrote.
pub const STUDIO_ADDON_VERSION: &str = "1.1";

/// The Sketchfab addon descriptor Godot reads, project-relative.
pub const SKETCHFAB_ADDON_CFG_REL: &str = "addons/bhippi_sketchfab/plugin.cfg";
/// The Sketchfab addon script, project-relative.
pub const SKETCHFAB_ADDON_SCRIPT_REL: &str = "addons/bhippi_sketchfab/plugin.gd";
/// The same descriptor as the `res://` path `[editor_plugins] enabled` lists.
pub const SKETCHFAB_ADDON_RES_PATH: &str = "res://addons/bhippi_sketchfab/plugin.cfg";
/// The Sketchfab addon's display name in Project Settings → Plugins.
pub const SKETCHFAB_ADDON_NAME: &str = "Bhippi Sketchfab";
/// The Sketchfab addon version (ADR-0055, GAD-181).
pub const SKETCHFAB_ADDON_VERSION: &str = "1.0";

/// Which starting point a new project gets.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum ProjectTemplate {
    /// A 3D scene with a light and a camera, and a root script.
    ///
    /// The aliases are the names a person — or the agent, in `<create_game>` — would write.
    /// serde's snake_case of `Empty3D` is `empty3_d`, which nobody types on purpose; the
    /// canonical name stays so the generated bindings do not move.
    #[default]
    #[serde(alias = "empty_3d", alias = "3d")]
    Empty3D,
    /// A walking, jumping `CharacterBody3D` on a floor.
    #[serde(alias = "third_person_3d", alias = "third_person")]
    ThirdPerson3D,
    /// A `CharacterBody2D` moving in four directions.
    #[serde(alias = "top_down_2d", alias = "2d")]
    TopDown2D,
}

impl ProjectTemplate {
    #[must_use]
    pub fn is_2d(self) -> bool {
        matches!(self, Self::TopDown2D)
    }

    /// The renderer Godot records in `config/features`.
    #[must_use]
    pub fn renderer_feature(self) -> &'static str {
        if self.is_2d() {
            "Mobile"
        } else {
            "Forward Plus"
        }
    }

    /// `renderer/rendering_method`, which must agree with the feature above.
    #[must_use]
    pub fn rendering_method(self) -> &'static str {
        if self.is_2d() {
            "mobile"
        } else {
            "forward_plus"
        }
    }

    #[must_use]
    pub fn pipeline(self) -> RenderPipeline {
        if self.is_2d() {
            RenderPipeline::D2d
        } else {
            RenderPipeline::D3d
        }
    }

    /// The script this template attaches, and where it goes.
    #[must_use]
    pub fn script_rel(self) -> &'static str {
        match self {
            Self::Empty3D => "scripts/main.gd",
            Self::ThirdPerson3D | Self::TopDown2D => "scripts/player.gd",
        }
    }
}

/// One file of a scaffolded project.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectFile {
    pub rel_path: String,
    pub contents: String,
}

/// The Bhippi studio editor addon: `plugin.cfg` and `plugin.gd`, in write order.
///
/// The studio viewport **is** this Godot editor window (ADR-0045), so the stock editor
/// layout spends the viewport on the Scene, FileSystem and Inspector docks. The addon turns
/// Godot's own distraction-free mode on once, at editor startup, and then leaves the user
/// alone — the docks come back with Ctrl+Shift+F12 or the toggle at the top right, and
/// nothing here takes them away again.
///
/// Since `1.1` it is also the editor's half of the live channel (GAD-170, ADR-0050): it
/// polls [`super::live::LIVE_SIGNAL_REL`] and, when Bhippi announces an applied batch,
/// rescans the filesystem, opens or reloads the scene the batch touched and selects the
/// nodes it wrote — so the viewport shows the work as it happens instead of whatever the
/// editor happened to be looking at. The constants on both sides are pinned to each other
/// by `the_addon_and_the_live_writer_agree_on_the_channel`.
///
/// This is Bhippi's own scaffold writing Bhippi's own files, the same class as
/// `bhippi/probe.gd`. INV-088 is about a *model* authoring project files and does not apply.
#[must_use]
pub fn studio_addon_files() -> Vec<ProjectFile> {
    vec![
        ProjectFile {
            rel_path: STUDIO_ADDON_CFG_REL.to_owned(),
            contents: studio_plugin_cfg().to_text(),
        },
        ProjectFile {
            rel_path: STUDIO_ADDON_SCRIPT_REL.to_owned(),
            contents: STUDIO_PLUGIN_SCRIPT.to_owned(),
        },
    ]
}

/// The Bhippi Sketchfab editor addon: `plugin.cfg` and `plugin.gd`, in write order.
///
/// A translucent strip along the bottom of the viewport holding the asset library
/// (ADR-0055). It is a **view over a file** and nothing more: it draws
/// [`super::sketchfab::LIBRARY_STATE_REL`] and posts clicks into
/// [`super::sketchfab::PANEL_REQUEST_REL`]. It makes no network request and holds no
/// credential, because a project folder is a thing people share and a token is not.
///
/// Same class of file as the studio addon and `bhippi/probe.gd`: Bhippi's own scaffold
/// writing Bhippi's own files, so INV-088 — which is about a *model* authoring project
/// files — does not apply.
#[must_use]
pub fn sketchfab_addon_files() -> Vec<ProjectFile> {
    vec![
        ProjectFile {
            rel_path: SKETCHFAB_ADDON_CFG_REL.to_owned(),
            contents: sketchfab_plugin_cfg().to_text(),
        },
        ProjectFile {
            rel_path: SKETCHFAB_ADDON_SCRIPT_REL.to_owned(),
            contents: SKETCHFAB_PLUGIN_SCRIPT.to_owned(),
        },
    ]
}

/// Every addon Bhippi installs into a project it manages, in write order.
#[must_use]
pub fn bhippi_addon_files() -> Vec<ProjectFile> {
    let mut files = studio_addon_files();
    files.extend(sketchfab_addon_files());
    files
}

/// The `res://` descriptor of every addon `[editor_plugins] enabled` must list.
pub const BHIPPI_ADDON_RES_PATHS: &[&str] = &[STUDIO_ADDON_RES_PATH, SKETCHFAB_ADDON_RES_PATH];

/// `plugin.cfg` — a Godot `ConfigFile`, so it is built from the same model that parses one
/// rather than from a format string that could drift out of the parser's grammar.
#[must_use]
fn studio_plugin_cfg() -> GodotIniFile {
    let mut file = GodotIniFile::default();
    let plugin = file.ensure_section("plugin");
    plugin.set("name", TscnValue::str(STUDIO_ADDON_NAME));
    plugin.set(
        "description",
        TscnValue::str(
            "Hides the editor docks so Bhippi's viewport shows the game (Ctrl+Shift+F12 \
             brings them back), and follows Bhippi's changes live: the scene it just edited \
             opens here, with the nodes it wrote selected.",
        ),
    );
    plugin.set("author", TscnValue::str("Bhippi"));
    plugin.set("version", TscnValue::str(STUDIO_ADDON_VERSION));
    // Godot resolves this relative to the plugin.cfg's own folder.
    plugin.set("script", TscnValue::str("plugin.gd"));
    file
}

/// `plugin.cfg` for the Sketchfab strip, built from the same `ConfigFile` model as the
/// studio addon's so both stay inside the grammar the parser accepts.
#[must_use]
fn sketchfab_plugin_cfg() -> GodotIniFile {
    let mut file = GodotIniFile::default();
    let plugin = file.ensure_section("plugin");
    plugin.set("name", TscnValue::str(SKETCHFAB_ADDON_NAME));
    plugin.set(
        "description",
        TscnValue::str(
            "A scrollable Sketchfab library along the bottom of the viewport. Search, see the              licence before you commit, and add a model to assets/ with one click. Bhippi does              the searching and downloading; this strip only draws it.",
        ),
    );
    plugin.set("author", TscnValue::str("Bhippi"));
    plugin.set("version", TscnValue::str(SKETCHFAB_ADDON_VERSION));
    plugin.set("script", TscnValue::str("plugin.gd"));
    file
}

/// Bring an existing project up to date with the studio addon. `true` when anything changed.
///
/// Idempotent, and deliberately narrow about what "up to date" means:
///
/// - each addon file is compared **byte for byte** with what [`bhippi_addon_files`] would
///   write, and rewritten when it is missing or different — so a newer Bhippi replaces an
///   older addon without a version handshake;
/// - `project.godot` is judged only on whether every path in [`BHIPPI_ADDON_RES_PATHS`] is
///   already in `[editor_plugins] enabled`. When they are, the file is not rewritten at all.
///   Godot writes this file too, and re-rendering a project the editor has since laid out
///   its own way, to change nothing, is how a round-trip bug becomes a corrupted project.
///
/// A project scaffolded before the Sketchfab strip existed therefore gains it on the next
/// workspace open, with no migration and no version handshake — the byte comparison is the
/// migration.
///
/// Errors are typed and carry the next step; the caller is expected to abort on them rather
/// than open the workspace with the docks in front of the viewport.
pub fn ensure_studio_addon(root: &Path) -> Result<bool> {
    // Read the project file first, so a folder that is not a Godot project is refused before
    // anything is created in it.
    let project_path = root.join(PROJECT_REL);
    let text = std::fs::read_to_string(&project_path).map_err(|error| EngineError::Io {
        operation: "studio addon",
        path: project_path.display().to_string(),
        reason: error.to_string(),
        hint: Some(
            "Godot identifies a project by project.godot; restore it or re-scaffold.".to_owned(),
        ),
    })?;
    let mut project = GodotProjectFile::parse(&text)?;

    let mut changed = false;
    for file in bhippi_addon_files() {
        let full = root.join(&file.rel_path);
        if std::fs::read(&full).ok().as_deref() == Some(file.contents.as_bytes()) {
            continue;
        }
        if let Some(parent) = full.parent() {
            std::fs::create_dir_all(parent).map_err(|error| EngineError::Io {
                operation: "studio addon",
                path: parent.display().to_string(),
                reason: error.to_string(),
                hint: Some("Check the project folder is writable.".to_owned()),
            })?;
        }
        std::fs::write(&full, &file.contents).map_err(|error| EngineError::Io {
            operation: "studio addon",
            path: full.display().to_string(),
            reason: error.to_string(),
            hint: Some(
                "Close the file in the Godot editor and check the project folder is writable."
                    .to_owned(),
            ),
        })?;
        changed = true;
    }

    let mut project_moved = false;
    for res_path in BHIPPI_ADDON_RES_PATHS {
        project_moved |= project.enable_editor_plugin(res_path);
    }
    if project_moved {
        std::fs::write(&project_path, project.to_text()).map_err(|error| EngineError::Io {
            operation: "studio addon",
            path: project_path.display().to_string(),
            reason: error.to_string(),
            hint: Some("Close project.godot in the Godot editor and try again.".to_owned()),
        })?;
        changed = true;
    }
    Ok(changed)
}

/// Every file a new project gets, in write order.
#[must_use]
pub fn plan(name: &str, template: ProjectTemplate) -> Vec<ProjectFile> {
    let manifest = godot_manifest(name, DEFAULT_MAIN_SCENE, template.pipeline());
    let mut files = vec![
        ProjectFile {
            rel_path: crate::GAME_MANIFEST_FILE.to_owned(),
            contents: render_manifest(&manifest),
        },
        ProjectFile {
            rel_path: PROJECT_REL.to_owned(),
            contents: project_file(name, template).to_text(),
        },
        ProjectFile {
            rel_path: MAIN_SCENE_REL.to_owned(),
            contents: main_scene(template).to_text(),
        },
        ProjectFile {
            rel_path: template.script_rel().to_owned(),
            contents: script_source(template).to_owned(),
        },
        ProjectFile {
            rel_path: PROBE_REL_PATH.to_owned(),
            contents: probe_source().to_owned(),
        },
        ProjectFile {
            rel_path: EXPORT_PRESETS_REL.to_owned(),
            contents: default_presets(name).to_text(),
        },
        ProjectFile {
            rel_path: ".gitignore".to_owned(),
            contents: GITIGNORE.to_owned(),
        },
        ProjectFile {
            rel_path: ICON_REL.to_owned(),
            contents: ICON_SVG.to_owned(),
        },
    ];
    files.extend(bhippi_addon_files());
    files
}

/// Create a Godot project at `root`. Returns the project-relative paths written.
///
/// Refuses a folder that already has anything in it unless `force`, for the same reason the
/// Bhippi scaffold does: "new project" over someone's work is not recoverable.
pub fn write_project(
    root: &Path,
    name: &str,
    template: ProjectTemplate,
    force: bool,
) -> Result<Vec<PathBuf>> {
    if name.trim().is_empty() {
        return Err(EngineError::Manifest(
            "a project needs a name".to_owned(),
            Some("Give the game a name; it becomes config/name in project.godot.".to_owned()),
        ));
    }
    if root.exists() {
        let mut entries = std::fs::read_dir(root)
            .map_err(|error| EngineError::Io {
                operation: "scaffold",
                path: root.display().to_string(),
                reason: error.to_string(),
                hint: Some("Check the target folder is readable.".to_owned()),
            })?
            .filter_map(std::result::Result::ok);
        if entries.next().is_some() && !force {
            return Err(EngineError::Manifest(
                format!("{} is not empty", root.display()),
                Some("Pick an empty folder or pass force=true.".to_owned()),
            ));
        }
    }
    std::fs::create_dir_all(root).map_err(|error| EngineError::Io {
        operation: "scaffold",
        path: root.display().to_string(),
        reason: error.to_string(),
        hint: Some("Check the parent folder is writable.".to_owned()),
    })?;

    let mut written = Vec::new();
    for file in plan(name, template) {
        let full = root.join(&file.rel_path);
        if let Some(parent) = full.parent() {
            std::fs::create_dir_all(parent).map_err(|error| EngineError::Io {
                operation: "scaffold",
                path: parent.display().to_string(),
                reason: error.to_string(),
                hint: None,
            })?;
        }
        if full.exists() && !force {
            return Err(EngineError::Manifest(
                format!("{} already exists", full.display()),
                Some("Pass force=true to overwrite.".to_owned()),
            ));
        }
        std::fs::write(&full, file.contents).map_err(|error| EngineError::Io {
            operation: "scaffold",
            path: full.display().to_string(),
            reason: error.to_string(),
            hint: Some("Check the project folder is writable.".to_owned()),
        })?;
        written.push(PathBuf::from(&file.rel_path));
    }
    if !root.join(".git").exists() {
        let _ = std::process::Command::new("git")
            .arg("init")
            .current_dir(root)
            .output();
    }
    Ok(written)
}

/// The asset folders every project is expected to have. Created empty, because an importer
/// that has to invent a directory tends to invent a different one each time.
pub const STANDARD_ASSET_DIRS: &[&str] = &["assets/models", "assets/textures", "assets/audio"];

/// Make `root` into a Godot project **without touching anything already in it**.
///
/// The difference from [`write_project`] is the whole point. That one refuses a folder with
/// files in it, and overwrites everything when forced — both correct for "new project", both
/// catastrophic here. This is for a folder that is *already someone's work* and is simply
/// missing the parts that make it a project: a `project.godot`, the studio addon, the asset
/// directories. Anything that exists is left exactly as it is.
///
/// The case it was written for: a project whose `project.godot` had never been created, whose
/// scenes and scripts the agent had been happily editing, and which the viewport therefore
/// refused with "is not a Godot project yet" every time it was opened — an error naming a
/// file the user had no way to produce.
///
/// Returns the project-relative paths it created, newest concern first. An empty vector means
/// the project was already complete, which is the common case and costs one `exists()` per
/// planned file.
///
/// # Errors
/// Fails only when the folder cannot be written to; a file that is already there is not an
/// error, it is the reason this function exists.
pub fn ensure_project(root: &Path, name: &str, template: ProjectTemplate) -> Result<Vec<PathBuf>> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(EngineError::Manifest(
            "a project needs a name".to_owned(),
            Some("Give the game a name; it becomes config/name in project.godot.".to_owned()),
        ));
    }
    std::fs::create_dir_all(root).map_err(|error| EngineError::Io {
        operation: "ensure project",
        path: root.display().to_string(),
        reason: error.to_string(),
        hint: Some("Check the parent folder is writable.".to_owned()),
    })?;

    let mut written = Vec::new();
    for file in plan(trimmed, template) {
        let full = root.join(&file.rel_path);
        // The one rule. A scene or a script the user has been working on outranks anything a
        // template would put at the same path.
        if full.exists() {
            continue;
        }
        if let Some(parent) = full.parent() {
            std::fs::create_dir_all(parent).map_err(|error| EngineError::Io {
                operation: "ensure project",
                path: parent.display().to_string(),
                reason: error.to_string(),
                hint: None,
            })?;
        }
        std::fs::write(&full, file.contents).map_err(|error| EngineError::Io {
            operation: "ensure project",
            path: full.display().to_string(),
            reason: error.to_string(),
            hint: Some("Check the project folder is writable.".to_owned()),
        })?;
        written.push(PathBuf::from(&file.rel_path));
    }

    for dir in STANDARD_ASSET_DIRS {
        let full = root.join(dir);
        if full.is_dir() {
            continue;
        }
        std::fs::create_dir_all(&full).map_err(|error| EngineError::Io {
            operation: "ensure project",
            path: full.display().to_string(),
            reason: error.to_string(),
            hint: Some("Check the project folder is writable.".to_owned()),
        })?;
        written.push(PathBuf::from(dir));
    }

    Ok(written)
}

/// The `project.godot` a template gets: renderer, autoload, input map and window size.
#[must_use]
pub fn project_file(name: &str, template: ProjectTemplate) -> GodotProjectFile {
    let mut project = GodotProjectFile::new(
        name,
        MAIN_SCENE_REL,
        &[FEATURE_VERSION, template.renderer_feature()],
    );
    project.set_icon(ICON_REL);
    // No engine splash on launch. The game is the user's; the first frame it shows is its
    // own scene on a plain dark ground, not a logo for the engine underneath (ADR-0047).
    project.file.set(
        "application",
        "boot_splash/show_image",
        TscnValue::Bool(false),
    );
    project.file.set(
        "application",
        "boot_splash/bg_color",
        TscnValue::Color(0.055, 0.055, 0.067, 1.0),
    );
    project.add_autoload(PROBE_AUTOLOAD_NAME, PROBE_RES_PATH, true);
    // The studio viewport is the editor itself, so a new project opens with its docks
    // hidden (ADR-0045), and with the Sketchfab strip along the bottom (ADR-0055).
    // Through the model, never by splicing the section into the text.
    for res_path in BHIPPI_ADDON_RES_PATHS {
        project.enable_editor_plugin(res_path);
    }

    project.file.set(
        "display",
        "window/size/viewport_width",
        TscnValue::Int(VIEWPORT_WIDTH),
    );
    project.file.set(
        "display",
        "window/size/viewport_height",
        TscnValue::Int(VIEWPORT_HEIGHT),
    );

    // WASD plus the arrow keys, which is what every player tries first.
    project.add_input_action("move_left", &[key::A, key::LEFT], DEADZONE);
    project.add_input_action("move_right", &[key::D, key::RIGHT], DEADZONE);
    project.add_input_action("move_forward", &[key::W, key::UP], DEADZONE);
    project.add_input_action("move_back", &[key::S, key::DOWN], DEADZONE);
    project.add_input_action(JUMP_ACTION, &[key::SPACE], DEADZONE);

    project.file.set(
        "rendering",
        "renderer/rendering_method",
        TscnValue::str(template.rendering_method()),
    );
    project
}

const DEADZONE: f64 = 0.5;

/// Godot 4 keycodes, as `OS.find_keycode_from_string` reports them.
mod key {
    pub const SPACE: u32 = 32;
    pub const A: u32 = 65;
    pub const D: u32 = 68;
    pub const S: u32 = 83;
    pub const W: u32 = 87;
    pub const LEFT: u32 = 4_194_319;
    pub const UP: u32 = 4_194_320;
    pub const RIGHT: u32 = 4_194_321;
    pub const DOWN: u32 = 4_194_322;
}

fn sub_resource(type_: &str, id: &str, properties: Vec<(&str, TscnValue)>) -> SubResource {
    SubResource {
        type_: type_.to_owned(),
        id: id.to_owned(),
        properties: properties
            .into_iter()
            .map(|(key, value)| (key.to_owned(), value))
            .collect(),
        order: Vec::new(),
    }
}

/// The starting scene for a template.
#[must_use]
pub fn main_scene(template: ProjectTemplate) -> TscnDocument {
    match template {
        ProjectTemplate::Empty3D => empty_3d(),
        ProjectTemplate::ThirdPerson3D => third_person_3d(),
        ProjectTemplate::TopDown2D => top_down_2d(),
    }
}

fn empty_3d() -> TscnDocument {
    let mut document = TscnDocument::new_scene("Main", "Node3D");
    let script = document.ensure_ext_resource("Script", "res://scripts/main.gd");
    if let Some(root) = document.nodes.first_mut() {
        root.set("script", TscnValue::ExtResource(script));
    }
    document.nodes.push(
        TscnNode::new("DirectionalLight3D", "DirectionalLight3D", Some("."))
            .with("transform", TscnValue::Raw(SUN_TRANSFORM.to_owned()))
            .with("shadow_enabled", TscnValue::Bool(true)),
    );
    document.nodes.push(
        TscnNode::new("Camera3D", "Camera3D", Some("."))
            .with("transform", TscnValue::Raw(CAMERA_TRANSFORM.to_owned()))
            .with("current", TscnValue::Bool(true)),
    );
    document.refresh_load_steps();
    document
}

fn third_person_3d() -> TscnDocument {
    let mut document = TscnDocument::new_scene("Main", "Node3D");
    let script = document.ensure_ext_resource("Script", "res://scripts/player.gd");
    document.sub_resources.push(sub_resource(
        "CapsuleShape3D",
        "CapsuleShape3D_player",
        vec![
            ("radius", TscnValue::Float(0.4)),
            ("height", TscnValue::Float(1.8)),
        ],
    ));
    document.sub_resources.push(sub_resource(
        "CapsuleMesh",
        "CapsuleMesh_player",
        vec![
            ("radius", TscnValue::Float(0.4)),
            ("height", TscnValue::Float(1.8)),
        ],
    ));
    document.sub_resources.push(sub_resource(
        "BoxShape3D",
        "BoxShape3D_floor",
        vec![("size", TscnValue::Vector3(20.0, 0.5, 20.0))],
    ));
    document.sub_resources.push(sub_resource(
        "BoxMesh",
        "BoxMesh_floor",
        vec![("size", TscnValue::Vector3(20.0, 0.5, 20.0))],
    ));

    document.nodes.push(
        TscnNode::new("DirectionalLight3D", "DirectionalLight3D", Some("."))
            .with("transform", TscnValue::Raw(SUN_TRANSFORM.to_owned()))
            .with("shadow_enabled", TscnValue::Bool(true)),
    );
    document.nodes.push(
        TscnNode::new("Camera3D", "Camera3D", Some("."))
            .with("transform", TscnValue::Raw(CAMERA_TRANSFORM.to_owned()))
            .with("current", TscnValue::Bool(true)),
    );
    document.nodes.push(
        TscnNode::new("Player", "CharacterBody3D", Some("."))
            .in_groups(&[super::scene::TRACK_GROUP])
            .with("transform", TscnValue::Raw(PLAYER_TRANSFORM.to_owned()))
            .with("script", TscnValue::ExtResource(script)),
    );
    document.nodes.push(
        TscnNode::new("CollisionShape3D", "CollisionShape3D", Some("Player")).with(
            "shape",
            TscnValue::SubResource("CapsuleShape3D_player".to_owned()),
        ),
    );
    document.nodes.push(
        TscnNode::new("MeshInstance3D", "MeshInstance3D", Some("Player")).with(
            "mesh",
            TscnValue::SubResource("CapsuleMesh_player".to_owned()),
        ),
    );
    document
        .nodes
        .push(TscnNode::new("Floor", "StaticBody3D", Some(".")).with(
            "transform",
            TscnValue::Raw("Transform3D(1, 0, 0, 0, 1, 0, 0, 0, 1, 0, -0.25, 0)".to_owned()),
        ));
    document.nodes.push(
        TscnNode::new("CollisionShape3D", "CollisionShape3D", Some("Floor")).with(
            "shape",
            TscnValue::SubResource("BoxShape3D_floor".to_owned()),
        ),
    );
    document.nodes.push(
        TscnNode::new("MeshInstance3D", "MeshInstance3D", Some("Floor"))
            .with("mesh", TscnValue::SubResource("BoxMesh_floor".to_owned())),
    );
    document.refresh_load_steps();
    document
}

fn top_down_2d() -> TscnDocument {
    let mut document = TscnDocument::new_scene("Main", "Node2D");
    let script = document.ensure_ext_resource("Script", "res://scripts/player.gd");
    document.sub_resources.push(sub_resource(
        "CircleShape2D",
        "CircleShape2D_player",
        vec![("radius", TscnValue::Float(16.0))],
    ));
    document.sub_resources.push(sub_resource(
        "RectangleShape2D",
        "RectangleShape2D_wall",
        vec![("size", TscnValue::Vector2(640.0, 32.0))],
    ));

    document.nodes.push(
        TscnNode::new("Camera2D", "Camera2D", Some("."))
            .with("position", TscnValue::Vector2(320.0, 180.0)),
    );
    document.nodes.push(
        TscnNode::new("Player", "CharacterBody2D", Some("."))
            .in_groups(&[super::scene::TRACK_GROUP])
            .with("position", TscnValue::Vector2(320.0, 180.0))
            .with("script", TscnValue::ExtResource(script)),
    );
    document.nodes.push(
        TscnNode::new("CollisionShape2D", "CollisionShape2D", Some("Player")).with(
            "shape",
            TscnValue::SubResource("CircleShape2D_player".to_owned()),
        ),
    );
    document.nodes.push(
        TscnNode::new("Polygon2D", "Polygon2D", Some("Player"))
            .with("color", TscnValue::Color(0.42, 0.72, 1.0, 1.0))
            .with(
                "polygon",
                TscnValue::Raw("PackedVector2Array(-16, -16, 16, -16, 16, 16, -16, 16)".to_owned()),
            ),
    );
    document.nodes.push(
        TscnNode::new("Wall", "StaticBody2D", Some("."))
            .with("position", TscnValue::Vector2(320.0, 340.0)),
    );
    document.nodes.push(
        TscnNode::new("CollisionShape2D", "CollisionShape2D", Some("Wall")).with(
            "shape",
            TscnValue::SubResource("RectangleShape2D_wall".to_owned()),
        ),
    );
    document.refresh_load_steps();
    document
}

/// The script a template attaches.
#[must_use]
pub fn script_source(template: ProjectTemplate) -> &'static str {
    match template {
        ProjectTemplate::Empty3D => EMPTY_3D_SCRIPT,
        ProjectTemplate::ThirdPerson3D => THIRD_PERSON_3D_SCRIPT,
        ProjectTemplate::TopDown2D => TOP_DOWN_2D_SCRIPT,
    }
}

const SUN_TRANSFORM: &str =
    "Transform3D(1, 0, 0, 0, 0.707107, 0.707107, 0, -0.707107, 0.707107, 0, 8, 0)";
const CAMERA_TRANSFORM: &str =
    "Transform3D(1, 0, 0, 0, 0.939693, 0.34202, 0, -0.34202, 0.939693, 0, 4, 8)";
const PLAYER_TRANSFORM: &str = "Transform3D(1, 0, 0, 0, 1, 0, 0, 0, 1, 0, 1, 0)";

const GITIGNORE: &str = "# Godot's import cache and editor state.\n\
                         .godot/\n\
                         \n\
                         # Exported builds.\n\
                         export/\n\
                         \n\
                         # Bhippi's own per-project state.\n\
                         .bhippi/\n";

/// A deliberately tiny icon: valid SVG, no external references, no licence question.
const ICON_SVG: &str = concat!(
    "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"128\" height=\"128\" viewBox=\"0 0 128 128\">\n",
    "  <rect width=\"128\" height=\"128\" rx=\"24\" fill=\"#1f2933\"/>\n",
    "  <circle cx=\"64\" cy=\"64\" r=\"34\" fill=\"#4fc3f7\"/>\n",
    "</svg>\n"
);

/// The studio addon's GDScript. In a file, not a Rust literal, for the reason `probe.rs`
/// gives: GDScript is indentation-sensitive and tabs inside `r#"…"#` are exactly what an
/// editor silently converts. `tests/godot_live.rs` runs `--check-only` over it.
const STUDIO_PLUGIN_SCRIPT: &str = include_str!("templates/studio_plugin.gd");
const SKETCHFAB_PLUGIN_SCRIPT: &str = include_str!("templates/sketchfab_plugin.gd");

const EMPTY_3D_SCRIPT: &str = include_str!("templates/main_empty_3d.gd");
const THIRD_PERSON_3D_SCRIPT: &str = include_str!("templates/player_third_person_3d.gd");
const TOP_DOWN_2D_SCRIPT: &str = include_str!("templates/player_top_down_2d.gd");

#[cfg(test)]
mod tests {
    use super::{
        bhippi_addon_files, ensure_studio_addon, main_scene, plan, script_source,
        sketchfab_addon_files, studio_addon_files, write_project, ProjectTemplate,
        BHIPPI_ADDON_RES_PATHS, EXPORT_PRESETS_REL, MAIN_SCENE_REL, PROJECT_REL,
        SKETCHFAB_ADDON_CFG_REL, SKETCHFAB_ADDON_SCRIPT_REL, STUDIO_ADDON_CFG_REL,
        STUDIO_ADDON_SCRIPT_REL,
    };
    use crate::godot::export_presets::ExportPresets;
    use crate::godot::live::{LIVE_POLL_MS, LIVE_SIGNAL_REL, LIVE_SIGNAL_VERSION};
    use crate::godot::project::{parse_ini, GodotProjectFile};
    use crate::godot::scene::GodotScene;
    use crate::godot::sketchfab::{
        CHANNEL_VERSION as SKETCHFAB_CHANNEL_VERSION, LIBRARY_STATE_REL as SKETCHFAB_STATE_REL,
        PANEL_POLL_MS as SKETCHFAB_POLL_MS, PANEL_REQUEST_REL as SKETCHFAB_REQUEST_REL,
    };
    use crate::godot::tscn::{self, TscnValue};
    use crate::manifest::parse_manifest;
    use std::path::PathBuf;

    struct TempRoot(PathBuf);

    impl TempRoot {
        fn new(name: &str) -> Self {
            let root = std::env::temp_dir().join(format!("bhippi-godot-scaffold-{name}"));
            let _ = std::fs::remove_dir_all(&root);
            Self(root)
        }
    }

    impl Drop for TempRoot {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    const ALL: [ProjectTemplate; 3] = [
        ProjectTemplate::Empty3D,
        ProjectTemplate::ThirdPerson3D,
        ProjectTemplate::TopDown2D,
    ];

    #[test]
    fn every_template_writes_a_project_that_parses_back() {
        for template in ALL {
            let root = TempRoot::new(&format!("{template:?}").to_lowercase());
            let written = write_project(&root.0, "Demo Game", template, false)
                .unwrap_or_else(|error| panic!("{template:?}: {error}"));

            for expected in [
                "Bhippi.game.toml",
                PROJECT_REL,
                MAIN_SCENE_REL,
                template.script_rel(),
                "bhippi/probe.gd",
                EXPORT_PRESETS_REL,
                ".gitignore",
                "icon.svg",
                STUDIO_ADDON_CFG_REL,
                STUDIO_ADDON_SCRIPT_REL,
            ] {
                assert!(
                    root.0.join(expected).is_file(),
                    "{template:?} must write {expected}"
                );
                assert!(written.contains(&PathBuf::from(expected)));
            }

            let manifest_text =
                std::fs::read_to_string(root.0.join("Bhippi.game.toml")).expect("manifest");
            let manifest = parse_manifest(&manifest_text).expect("manifest parses");
            assert!(crate::godot::manifest::is_godot(&manifest));

            let project_text = std::fs::read_to_string(root.0.join(PROJECT_REL)).expect("project");
            let project = GodotProjectFile::parse(&project_text).expect("project parses");
            assert_eq!(
                project.main_scene().as_deref(),
                Some("res://scenes/main.tscn")
            );
            // No engine splash: the game's first frame is its own scene (ADR-0047 §5).
            assert!(
                project_text.contains("boot_splash/show_image=false"),
                "{template:?} must turn the boot splash off"
            );
            assert!(root
                .0
                .join(crate::godot::res_to_rel(
                    &project.main_scene().unwrap_or_default()
                ))
                .is_file());
            assert!(project
                .autoloads()
                .iter()
                .any(|autoload| autoload.name == "BhippiProbe" && autoload.singleton));
            let mut actions = project.input_actions();
            actions.sort();
            assert_eq!(
                actions,
                vec![
                    "jump",
                    "move_back",
                    "move_forward",
                    "move_left",
                    "move_right"
                ]
            );
            assert!(project
                .features()
                .contains(&template.renderer_feature().to_owned()));
            assert_eq!(
                project.editor_plugins(),
                BHIPPI_ADDON_RES_PATHS
                    .iter()
                    .map(|path| (*path).to_owned())
                    .collect::<Vec<_>>(),
                "{template:?} must open with every Bhippi addon enabled"
            );

            let scene_text = std::fs::read_to_string(root.0.join(MAIN_SCENE_REL)).expect("scene");
            let document = tscn::parse(&scene_text).expect("scene parses");
            assert_eq!(document.to_text(), scene_text, "the scene round-trips");
            let scene = GodotScene::from_document(document);
            for script in scene.scripts() {
                assert!(
                    root.0.join(crate::godot::res_to_rel(&script)).is_file(),
                    "{template:?} references {script} but did not write it"
                );
            }

            let presets_text =
                std::fs::read_to_string(root.0.join(EXPORT_PRESETS_REL)).expect("presets");
            assert!(ExportPresets::parse(&presets_text)
                .expect("presets parse")
                .has_preset("Web"));
        }
    }

    #[test]
    fn the_third_person_player_is_a_real_character_body() {
        let source = script_source(ProjectTemplate::ThirdPerson3D);
        for needle in [
            "extends CharacterBody3D",
            "move_and_slide()",
            "is_on_floor()",
            "physics/3d/default_gravity",
            "Input.get_vector(\"move_left\", \"move_right\", \"move_forward\", \"move_back\")",
            "Input.is_action_just_pressed(\"jump\")",
            // Looked up, never named: --check-only does not register autoloads.
            "get_node_or_null(\"/root/BhippiProbe\")",
            "_probe.set_var(\"player_y\"",
        ] {
            assert!(source.contains(needle), "player.gd must contain `{needle}`");
        }
        let scene = GodotScene::from_document(main_scene(ProjectTemplate::ThirdPerson3D));
        assert_eq!(scene.find_by_type("CharacterBody3D"), vec!["Player"]);
        assert_eq!(scene.tracked(), vec!["Player"]);
        assert!(scene.contains("Player/CollisionShape3D"));
        assert!(scene.contains("Floor/MeshInstance3D"));
    }

    #[test]
    fn the_two_dimensional_template_is_two_dimensional_all_the_way_through() {
        let template = ProjectTemplate::TopDown2D;
        assert!(template.is_2d());
        assert_eq!(template.renderer_feature(), "Mobile");
        assert_eq!(template.rendering_method(), "mobile");
        let source = script_source(template);
        assert!(source.contains("extends CharacterBody2D"));
        assert!(source.contains("move_and_slide()"));
        assert!(
            !source.contains("\tBhippiProbe."),
            "autoloads are looked up, not named"
        );
        let scene = GodotScene::from_document(main_scene(template));
        assert_eq!(
            scene.root().and_then(|node| node.type_.clone()),
            Some("Node2D".to_owned())
        );
        assert_eq!(scene.find_by_type("CharacterBody2D"), vec!["Player"]);
    }

    #[test]
    fn every_scaffolded_scene_reports_the_load_steps_it_actually_has() {
        for template in ALL {
            let document = main_scene(template);
            assert_eq!(
                document.header.load_steps,
                document.computed_load_steps(),
                "{template:?}"
            );
        }
    }

    #[test]
    fn a_non_empty_folder_is_refused_unless_forced() {
        let root = TempRoot::new("guard");
        std::fs::create_dir_all(&root.0).expect("root");
        std::fs::write(root.0.join(".placeholder"), "x").expect("placeholder");

        let refused = write_project(&root.0, "Demo", ProjectTemplate::Empty3D, false)
            .expect_err("must refuse");
        assert!(refused.hint().is_some());
        assert!(!root.0.join(PROJECT_REL).exists());

        write_project(&root.0, "Demo", ProjectTemplate::Empty3D, true).expect("force works");
        assert!(root.0.join(PROJECT_REL).is_file());
    }

    #[test]
    fn a_nameless_project_is_refused_before_anything_is_written() {
        let root = TempRoot::new("noname");
        let error = write_project(&root.0, "   ", ProjectTemplate::Empty3D, false)
            .expect_err("must refuse");
        assert!(error.hint().is_some());
        assert!(
            !root.0.exists(),
            "nothing may be created for a refused name"
        );
    }

    #[test]
    fn the_studio_addon_is_exactly_these_bytes() {
        let files = studio_addon_files();
        assert_eq!(files.len(), 2);
        assert_eq!(files[0].rel_path, STUDIO_ADDON_CFG_REL);
        assert_eq!(files[1].rel_path, STUDIO_ADDON_SCRIPT_REL);

        // plugin.cfg, in the exact shape Godot's own plugin dialog writes.
        assert_eq!(
            files[0].contents,
            concat!(
                "[plugin]\n",
                "\n",
                "name=\"Bhippi Studio\"\n",
                "description=\"Hides the editor docks so Bhippi's viewport shows the game (Ctrl+Shift+F12 brings them back), and follows Bhippi's changes live: the scene it just edited opens here, with the nodes it wrote selected.\"\n",
                "author=\"Bhippi\"\n",
                "version=\"1.1\"\n",
                "script=\"plugin.gd\"\n",
            )
        );
        // A plugin.cfg is a ConfigFile, so the file the scaffold writes is one the parser
        // in this crate reads back unchanged.
        let reparsed = parse_ini(&files[0].contents).expect("plugin.cfg parses");
        assert_eq!(reparsed.to_text(), files[0].contents);
        assert_eq!(
            reparsed
                .get("plugin", "script")
                .and_then(TscnValue::as_str)
                .map(str::to_owned),
            Some("plugin.gd".to_owned())
        );

        let script = &files[1].contents;
        for needle in [
            "@tool\n",
            "extends EditorPlugin",
            "func _enter_tree() -> void:",
            // ADR-0064: the docks follow preview mode, which the user drives from the
            // editor's own toolbar rather than by a value set once at load.
            "EditorInterface.set_distraction_free_mode(_preview_on)",
            "add_control_to_container(CONTAINER_TOOLBAR, _toolbar)",
            "Ctrl+Shift+F12",
            // GAD-170: the live follower, and the three editor calls that are the whole of it.
            "func _poll() -> void:",
            "EditorInterface.get_resource_filesystem()",
            "EditorInterface.open_scene_from_path(scene)",
            "EditorInterface.reload_scene_from_path(scene)",
            "selection.add_node(node)",
        ] {
            assert!(script.contains(needle), "plugin.gd must contain `{needle}`");
        }
        // The addon polls the live signal — that is the mechanism — but the distraction-free
        // mode is still set exactly once, from `_enter_tree` and nowhere else, so the user's
        // own Ctrl+Shift+F12 is never undone a quarter of a second later.
        assert_eq!(
            script.matches("set_distraction_free_mode").count(),
            1,
            "the mode is set once and never re-asserted:\n{script}"
        );
        let after_poll = script.split("func _poll()").nth(1).unwrap_or_default();
        assert!(
            !after_poll.contains("set_distraction_free_mode"),
            "nothing below _poll may touch the docks"
        );
        // ADR-0064: the menus are hidden one button at a time. Godot 4 keeps them in the
        // same title-bar container as the main-screen switcher, so hiding the *container*
        // would take 2D, 3D, Script and Game with it — the things preview mode keeps.
        assert!(
            script.contains(r#"["Scene", "Project", "Debug", "Editor", "Help"]"#),
            "the menu names are named, so the walk cannot drift"
        );
        for kept in ["\"2D\"", "\"3D\"", "\"Script\"", "\"AssetLib\""] {
            assert!(
                !script.contains(kept),
                "a GDScript string {kept} must never appear on a hide list"
            );
        }
        // The Play button asks Bhippi rather than running the game itself: a window the app
        // did not launch cannot be re-parented into the viewport.
        assert!(script.contains("PLAY_REQUEST_REL"));
        assert!(
            !script.contains("EditorInterface.play_main_scene"),
            "the addon never runs the game itself"
        );
        assert!(!script.contains("\r\n"), "GDScript is LF-terminated");
        assert!(
            script.lines().all(|line| !line.starts_with(' ')),
            "GDScript is tab-indented; no line may start with a space"
        );
    }

    /// The addon and `godot::live` are two halves of one channel written in two languages.
    /// Nothing but this test stops the Rust side moving the file and the GDScript side
    /// quietly reading a path that no longer exists — which would look exactly like an
    /// editor that had stopped following, with no error anywhere.
    #[test]
    fn the_addon_and_the_live_writer_agree_on_the_channel() {
        let script = &studio_addon_files()[1].contents;
        assert!(
            script.contains(&format!("const SIGNAL_REL := \"{LIVE_SIGNAL_REL}\"")),
            "the addon must read the path `godot::live` writes"
        );
        assert!(
            script.contains(&format!("const SIGNAL_VERSION := {LIVE_SIGNAL_VERSION}")),
            "the addon must accept the version `godot::live` stamps"
        );
        let poll_seconds = f64::from(LIVE_POLL_MS) / 1000.0;
        assert!(
            script.contains(&format!("const POLL_SECONDS := {poll_seconds}")),
            "the addon must poll at LIVE_POLL_MS ({LIVE_POLL_MS} ms = {poll_seconds} s)"
        );
    }

    /// The same pinning for the Sketchfab strip. The panel is a view over two files whose
    /// paths, version and poll interval are declared in Rust; if the Rust side moves one and
    /// the GDScript side is not updated, the strip silently draws nothing for ever and the
    /// person concludes Sketchfab is broken.
    #[test]
    fn the_addon_and_the_sketchfab_channel_agree() {
        let files = sketchfab_addon_files();
        assert_eq!(files.len(), 2);
        assert_eq!(files[0].rel_path, SKETCHFAB_ADDON_CFG_REL);
        assert_eq!(files[1].rel_path, SKETCHFAB_ADDON_SCRIPT_REL);

        // The descriptor Godot reads must be a ConfigFile the project parser accepts, and
        // it must point at the script that sits beside it.
        let cfg = parse_ini(&files[0].contents).expect("plugin.cfg parses");
        let plugin = cfg.section("plugin").expect("[plugin] section");
        assert_eq!(
            plugin.get("script").map(tscn::TscnValue::to_text),
            Some("\"plugin.gd\"".to_owned())
        );
        assert_eq!(
            plugin.get("name").map(tscn::TscnValue::to_text),
            Some("\"Bhippi Sketchfab\"".to_owned())
        );

        let script = &files[1].contents;
        assert!(
            script.contains(&format!("const STATE_REL := \"{SKETCHFAB_STATE_REL}\"")),
            "the panel must read the path `godot::sketchfab` writes"
        );
        assert!(
            script.contains(&format!("const REQUEST_REL := \"{SKETCHFAB_REQUEST_REL}\"")),
            "the panel must write the path `godot::sketchfab` takes from"
        );
        assert!(
            script.contains(&format!(
                "const CHANNEL_VERSION := {SKETCHFAB_CHANNEL_VERSION}"
            )),
            "the panel must accept the version `godot::sketchfab` stamps"
        );
        let poll_seconds = f64::from(SKETCHFAB_POLL_MS) / 1000.0;
        assert!(
            script.contains(&format!("const POLL_SECONDS := {poll_seconds}")),
            "the panel must poll at PANEL_POLL_MS ({SKETCHFAB_POLL_MS} ms = {poll_seconds} s)"
        );

        // Every `action` word the panel can post has to be one `PanelRequest` parses. A
        // typo here is a button that does nothing at all, which is the hardest kind of bug
        // to see in a screenshot.
        for action in ["connect", "disconnect", "search", "import", "open"] {
            assert!(
                script.contains(&format!("\"action\": \"{action}\"")),
                "the panel should be able to post `{action}`"
            );
            let payload = match action {
                "search" => serde_json::json!({"action": action, "query": "x"}),
                "import" | "open" => serde_json::json!({"action": action, "uid": "u"}),
                _ => serde_json::json!({"action": action}),
            };
            serde_json::from_value::<crate::godot::sketchfab::PanelRequest>(payload)
                .unwrap_or_else(|error| panic!("`{action}` must parse as a PanelRequest: {error}"));
        }

        // The panel decides nothing: no HTTP, no credential, no licence rule in GDScript.
        for forbidden in [
            "HTTPRequest",
            "HTTPClient",
            "Authorization",
            "api.sketchfab.com",
        ] {
            assert!(
                !script.contains(forbidden),
                "the panel is a view over a file; `{forbidden}` does not belong in it"
            );
        }

        assert!(!script.contains("\r\n"), "GDScript is LF-terminated");
        assert!(
            script.lines().all(|line| !line.starts_with(' ')),
            "GDScript is tab-indented; no line may start with a space"
        );
    }

    #[test]
    fn ensure_studio_addon_installs_once_and_then_does_nothing() {
        let root = TempRoot::new("ensure-addon");
        write_project(&root.0, "Older Project", ProjectTemplate::Empty3D, false).expect("scaffold");

        // Age the project back to before the addon existed: no addons folder, and no
        // [editor_plugins] section in project.godot.
        std::fs::remove_dir_all(root.0.join("addons")).expect("remove addons");
        let project_path = root.0.join(PROJECT_REL);
        let mut aged = GodotProjectFile::parse(
            &std::fs::read_to_string(&project_path).expect("project reads"),
        )
        .expect("project parses");
        assert!(aged.file.remove("editor_plugins", "enabled"));
        let before = aged.to_text();
        std::fs::write(&project_path, &before).expect("aged project writes");

        // First call installs.
        assert!(ensure_studio_addon(&root.0).expect("first call"));
        for file in bhippi_addon_files() {
            assert_eq!(
                std::fs::read_to_string(root.0.join(&file.rel_path)).expect(&file.rel_path),
                file.contents,
                "{} must be written verbatim",
                file.rel_path
            );
        }
        let after = std::fs::read_to_string(&project_path).expect("project reads");
        assert!(after.contains(
            "[editor_plugins]\n\nenabled=PackedStringArray(\"res://addons/bhippi_studio/plugin.cfg\", \"res://addons/bhippi_sketchfab/plugin.cfg\")\n"
        ), "unexpected project.godot:\n{after}");

        // …and touched nothing else: drop the one key it added and the bytes are the old ones.
        let mut stripped = GodotProjectFile::parse(&after).expect("re-parses");
        assert!(stripped.file.remove("editor_plugins", "enabled"));
        assert_eq!(stripped.to_text(), before);

        // Second call is a no-op, down to the bytes on disk.
        assert!(!ensure_studio_addon(&root.0).expect("second call"));
        assert_eq!(
            std::fs::read_to_string(&project_path).expect("project reads"),
            after
        );
    }

    #[test]
    fn ensure_studio_addon_repairs_an_edited_or_missing_addon_file() {
        let root = TempRoot::new("repair-addon");
        write_project(&root.0, "Scaffolded", ProjectTemplate::Empty3D, false).expect("scaffold");
        // A freshly scaffolded project already has it, so there is nothing to do.
        assert!(!ensure_studio_addon(&root.0).expect("fresh scaffold is up to date"));

        // An older or hand-edited copy is replaced, byte for byte.
        std::fs::write(
            root.0.join(STUDIO_ADDON_SCRIPT_REL),
            "@tool\nextends Node\n",
        )
        .expect("stale script");
        assert!(ensure_studio_addon(&root.0).expect("repairs the script"));
        assert_eq!(
            std::fs::read_to_string(root.0.join(STUDIO_ADDON_SCRIPT_REL)).expect("script"),
            studio_addon_files()[1].contents
        );

        // A deleted descriptor comes back without disturbing project.godot.
        let project_before =
            std::fs::read_to_string(root.0.join(PROJECT_REL)).expect("project reads");
        std::fs::remove_file(root.0.join(STUDIO_ADDON_CFG_REL)).expect("remove cfg");
        assert!(ensure_studio_addon(&root.0).expect("restores the descriptor"));
        assert!(root.0.join(STUDIO_ADDON_CFG_REL).is_file());
        assert_eq!(
            std::fs::read_to_string(root.0.join(PROJECT_REL)).expect("project reads"),
            project_before,
            "project.godot must not be rewritten when the plugin is already enabled"
        );
    }

    #[test]
    fn ensure_studio_addon_refuses_a_folder_that_is_not_a_godot_project() {
        let root = TempRoot::new("not-a-project");
        std::fs::create_dir_all(&root.0).expect("root");
        let error = ensure_studio_addon(&root.0).expect_err("no project.godot must fail");
        assert!(error.hint().is_some(), "a refusal carries the next step");
    }

    #[test]
    fn the_plan_is_deterministic() {
        let first = plan("Demo", ProjectTemplate::ThirdPerson3D);
        let second = plan("Demo", ProjectTemplate::ThirdPerson3D);
        // Only the manifest's freshly minted GameId differs between two plans.
        let strip = |files: &[super::ProjectFile]| -> Vec<(String, String)> {
            files
                .iter()
                .filter(|file| file.rel_path != crate::GAME_MANIFEST_FILE)
                .map(|file| (file.rel_path.clone(), file.contents.clone()))
                .collect()
        };
        assert_eq!(strip(&first), strip(&second));
    }

    // ── filling in a project that is missing its parts (ADR-0066) ───────────────────
    //
    // The owner's project had scenes and scripts the agent had been editing, and no
    // `project.godot` — so the viewport refused it every time with "is not a Godot project
    // yet: there is no project.godot", naming a file he had no way to produce.

    #[test]
    fn a_bare_folder_becomes_a_project() {
        let root = TempRoot::new("ensure-bare");
        let made = super::ensure_project(&root.0, "racoon boat", ProjectTemplate::Empty3D)
            .expect("a bare folder can be filled in");
        assert!(
            root.0.join("project.godot").is_file(),
            "the file that was missing"
        );
        assert!(!made.is_empty());
        for dir in super::STANDARD_ASSET_DIRS {
            assert!(root.0.join(dir).is_dir(), "{dir} was not created");
        }
    }

    #[test]
    fn existing_work_is_never_overwritten() {
        // The rule this function exists for. `write_project(force: true)` overwrites every
        // planned path, which here would mean replacing the scenes and scripts the agent had
        // spent the session on with an empty template — a worse outcome than the refusal.
        let root = TempRoot::new("ensure-keeps");
        let scripts = root.0.join("scripts");
        std::fs::create_dir_all(&scripts).expect("temp dirs");
        let mine = scripts.join("main.gd");
        std::fs::write(
            &mine,
            "# the owner's own script
extends Node3D
",
        )
        .expect("seed");

        // A file at a path the template also plans for.
        let planned = super::plan("racoon boat", ProjectTemplate::Empty3D);
        let clash = planned
            .iter()
            .find(|file| file.rel_path.ends_with(".gd") || file.rel_path.ends_with(".tscn"))
            .expect("the template plans at least one scene or script");
        let clash_path = root.0.join(&clash.rel_path);
        if let Some(parent) = clash_path.parent() {
            std::fs::create_dir_all(parent).expect("parent");
        }
        std::fs::write(&clash_path, "KEEP ME").expect("seed the clash");

        super::ensure_project(&root.0, "racoon boat", ProjectTemplate::Empty3D).expect("fills in");

        assert_eq!(
            std::fs::read_to_string(&mine).expect("still there"),
            "# the owner's own script
extends Node3D
",
            "an unrelated file was touched"
        );
        assert_eq!(
            std::fs::read_to_string(&clash_path).expect("still there"),
            "KEEP ME",
            "a file the template also wanted was overwritten"
        );
        assert!(
            root.0.join("project.godot").is_file(),
            "and the missing part was still made"
        );
    }

    #[test]
    fn a_second_pass_changes_nothing() {
        // Every workspace open runs this. If it were not idempotent it would rewrite the
        // project under the editor on every launch.
        let root = TempRoot::new("ensure-twice");
        super::ensure_project(&root.0, "twice", ProjectTemplate::Empty3D).expect("first");
        let made =
            super::ensure_project(&root.0, "twice", ProjectTemplate::Empty3D).expect("second");
        assert!(made.is_empty(), "the second pass created {made:?}");
    }

    #[test]
    fn a_nameless_project_is_refused_rather_than_guessed() {
        let root = TempRoot::new("ensure-nameless");
        assert!(super::ensure_project(&root.0, "   ", ProjectTemplate::Empty3D).is_err());
    }

    #[test]
    fn the_viewport_fills_a_project_in_before_it_decides_it_is_one() {
        // Ordering matters: the `is_godot_project` check that drives the refusal has to be
        // asked *after* the scaffold, or the project is made and refused in the same breath.
        let embed = include_str!("../../../bhippi-app/src/godot_embed.rs");
        let at = embed
            .find("ensure_project(")
            .expect("the viewport fills projects in");
        let decided = embed
            .find("let is_godot_project = root.join(\"project.godot\").is_file();")
            .expect("the check is still there");
        assert!(
            at < decided,
            "the scaffold must run before the check that refuses"
        );
    }
}
