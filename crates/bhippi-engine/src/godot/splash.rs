//! The splash screen: the card a game shows before it starts (GAD-161).
//!
//! A game boots into its splash, holds it for a few seconds, and hands over to the scene it
//! would otherwise have started on. That handover is the whole design constraint: the splash
//! is the project's **main scene**, so it must know where to go next and must never be able
//! to point at itself. A boot loop is not a cosmetic bug — the game never starts.
//!
//! Everything here is pure. The spec is synthesised from what the user wrote, validated
//! against gates that *block*, and lowered into a typed [`GodotActionBatch`] — so the agent
//! never hand-writes a `.tscn`, `project.godot` or a raw script body (INV-088, R8). The
//! GDScript this emits still goes through `--check-only` at apply time like every other
//! script, which is what makes "it compiles" a fact rather than a hope.
//!
//! The same spec renders to SVG ([`svg`]) so a splash can leave the studio as a file any
//! other tool can open. That path never runs Godot: an export the user cannot get without a
//! working engine install is an export that fails when they most want it.

use super::action::{GodotAction, GodotActionBatch};
use super::tscn::TscnValue;
use crate::error::{EngineError, Result};
use serde::{Deserialize, Serialize};
use specta::Type;
use std::path::Path;

/// Where the generated splash scene goes.
pub const SPLASH_SCENE_REL: &str = "scenes/splash.tscn";
/// Where the script that times the splash and hands over goes.
pub const SPLASH_SCRIPT_REL: &str = "scripts/splash.gd";
/// Where an uploaded logo is kept, project-relative.
pub const SPLASH_LOGO_DIR: &str = "assets/ui/splash";
/// Where the spec is kept so the panel can reopen what was built.
///
/// Under `.bhippi/` because Godot's importer ignores dot-directories, so this never becomes
/// a stray resource in the user's project tree.
pub const SPLASH_SPEC_REL: &str = ".bhippi/splash.json";

/// The root node of the generated scene.
pub const SPLASH_NODE_NAME: &str = "Splash";
/// Above ordinary game canvases; the splash covers everything while it runs.
pub const SPLASH_CANVAS_LAYER: i64 = 128;

/// The shortest a splash may hold. Under this it reads as a flicker rather than a title.
pub const MIN_SPLASH_MS: u32 = 3_000;
/// The longest a splash may hold. Past this it is something the player waits through.
pub const MAX_SPLASH_MS: u32 = 5_000;
/// What a splash holds for when the brief does not say.
pub const DEFAULT_SPLASH_MS: u32 = 3_500;

/// The longest brief accepted. A description, not a design document.
pub const MAX_BRIEF_CHARS: usize = 2_000;
/// The longest title a splash will letter.
pub const MAX_TITLE_CHARS: usize = 64;
/// The longest tagline a splash will letter.
pub const MAX_TAGLINE_CHARS: usize = 96;

/// The smallest the title may be lettered, at the 720p reference height.
pub const MIN_TITLE_FONT: i64 = 40;
/// The smallest the tagline may be lettered.
pub const MIN_TAGLINE_FONT: i64 = 16;

/// The least contrast allowed between the lettering and the backdrop (WCAG AA for large
/// text). A splash nobody can read is the one failure mode that survives every review,
/// because the person who built it already knows what it says.
pub const MIN_CONTRAST: f64 = 3.0;

/// How the splash animates in and out.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum SplashMotion {
    /// Opacity only. The safe one, and the default.
    #[default]
    Fade,
    /// Fades while lifting slightly. Reads as "premium" without costing legibility.
    Rise,
    /// Fades while scaling up from just under full size.
    Zoom,
    /// Nothing moves. For a deliberately flat, printed look.
    Still,
}

impl SplashMotion {
    /// Every motion, for the panel's picker.
    #[must_use]
    pub const fn all() -> &'static [Self] {
        &[Self::Fade, Self::Rise, Self::Zoom, Self::Still]
    }

    /// The stable id this serialises as.
    #[must_use]
    pub const fn id(self) -> &'static str {
        match self {
            Self::Fade => "fade",
            Self::Rise => "rise",
            Self::Zoom => "zoom",
            Self::Still => "still",
        }
    }

    /// What the picker calls it.
    #[must_use]
    pub const fn title(self) -> &'static str {
        match self {
            Self::Fade => "Fade",
            Self::Rise => "Rise",
            Self::Zoom => "Zoom",
            Self::Still => "Still",
        }
    }

    fn from_id(id: &str) -> Option<Self> {
        Self::all().iter().copied().find(|entry| entry.id() == id)
    }
}

/// What sits behind the lettering.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum SplashBackdrop {
    /// One flat colour.
    #[default]
    Solid,
    /// The background colour, darkened towards the edges.
    Vignette,
    /// A band of the accent colour behind the lettering.
    Band,
}

impl SplashBackdrop {
    /// Every backdrop, for the panel's picker.
    #[must_use]
    pub const fn all() -> &'static [Self] {
        &[Self::Solid, Self::Vignette, Self::Band]
    }

    /// The stable id this serialises as.
    #[must_use]
    pub const fn id(self) -> &'static str {
        match self {
            Self::Solid => "solid",
            Self::Vignette => "vignette",
            Self::Band => "band",
        }
    }

    /// What the picker calls it.
    #[must_use]
    pub const fn title(self) -> &'static str {
        match self {
            Self::Solid => "Solid",
            Self::Vignette => "Vignette",
            Self::Band => "Band",
        }
    }

    fn from_id(id: &str) -> Option<Self> {
        Self::all().iter().copied().find(|entry| entry.id() == id)
    }
}

/// The three colours a splash is drawn from, as `#rrggbb`.
///
/// Hex rather than floats because these cross IPC to a panel that paints swatches with them,
/// and a colour the user can read in the same notation the rest of the world uses is one
/// they can paste from a brand guide.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize, Type)]
pub struct SplashPalette {
    pub background: String,
    pub ink: String,
    pub accent: String,
}

impl Default for SplashPalette {
    fn default() -> Self {
        Self {
            background: "#0b0f19".to_owned(),
            ink: "#f5f7fa".to_owned(),
            accent: "#5b8cff".to_owned(),
        }
    }
}

/// One splash screen, completely described.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct SplashSpec {
    /// What the user asked for, kept so the panel can reopen and re-generate it.
    pub brief: String,
    pub title: String,
    pub tagline: String,
    pub palette: SplashPalette,
    pub motion: SplashMotion,
    pub backdrop: SplashBackdrop,
    /// How long the splash holds, in milliseconds. Always within [`MIN_SPLASH_MS`]…
    /// [`MAX_SPLASH_MS`].
    pub duration_ms: u32,
    /// `res://` path of the logo, when one was uploaded.
    pub logo_res_path: Option<String>,
    pub title_font_size: i64,
    pub tagline_font_size: i64,
    /// Which mood the brief was read as, or `custom` when it matched none. Shown in the
    /// panel so the palette reads as a decision the brief caused rather than a random one.
    pub mood: String,
}

impl SplashSpec {
    /// True when this splash letters anything. A logo-only card does not.
    #[must_use]
    pub fn has_text(&self) -> bool {
        !self.title.trim().is_empty() || !self.tagline.trim().is_empty()
    }

    /// True when there is anything on the card at all.
    #[must_use]
    pub fn has_content(&self) -> bool {
        self.has_text()
            || self
                .logo_res_path
                .as_deref()
                .is_some_and(|path| !path.trim().is_empty())
    }
}

impl Default for SplashSpec {
    fn default() -> Self {
        Self {
            brief: String::new(),
            title: "Untitled".to_owned(),
            tagline: String::new(),
            palette: SplashPalette::default(),
            motion: SplashMotion::Fade,
            backdrop: SplashBackdrop::Solid,
            duration_ms: DEFAULT_SPLASH_MS,
            logo_res_path: None,
            title_font_size: 72,
            tagline_font_size: 24,
            mood: "custom".to_owned(),
        }
    }
}

/// One named palette the synthesiser can reach for, and the words that ask for it.
struct Mood {
    id: &'static str,
    words: &'static [&'static str],
    background: &'static str,
    ink: &'static str,
    accent: &'static str,
    motion: SplashMotion,
    backdrop: SplashBackdrop,
}

/// The moods, in the order they are matched.
///
/// This is a lookup, not a model call: the panel must answer instantly and must answer with
/// no network, no provider and no key. A brief that matches nothing lands on the default,
/// which is a legible dark card rather than a failure.
const MOODS: &[Mood] = &[
    Mood {
        id: "neon",
        words: &[
            "neon",
            "cyberpunk",
            "synthwave",
            "vapor",
            "arcade",
            "retrowave",
        ],
        background: "#12002e",
        ink: "#f6f0ff",
        accent: "#ff2fb9",
        motion: SplashMotion::Zoom,
        backdrop: SplashBackdrop::Vignette,
    },
    Mood {
        id: "forest",
        words: &[
            "forest", "nature", "woodland", "jungle", "earth", "cozy", "cosy",
        ],
        background: "#10241a",
        ink: "#f2f7ef",
        accent: "#7bc96f",
        motion: SplashMotion::Rise,
        backdrop: SplashBackdrop::Vignette,
    },
    Mood {
        id: "ember",
        words: &[
            "fire", "ember", "lava", "volcano", "hell", "infernal", "warm",
        ],
        background: "#1d0b06",
        ink: "#fff3e8",
        accent: "#ff6b35",
        motion: SplashMotion::Zoom,
        backdrop: SplashBackdrop::Vignette,
    },
    Mood {
        id: "ice",
        words: &[
            "ice", "frost", "winter", "arctic", "snow", "glacier", "cold",
        ],
        background: "#07202e",
        ink: "#eefaff",
        accent: "#5ad2f4",
        motion: SplashMotion::Fade,
        backdrop: SplashBackdrop::Vignette,
    },
    Mood {
        id: "paper",
        words: &[
            "paper", "minimal", "clean", "light", "bright", "simple", "flat",
        ],
        background: "#f4f1ea",
        ink: "#1b1a17",
        accent: "#c2410c",
        motion: SplashMotion::Fade,
        backdrop: SplashBackdrop::Solid,
    },
    Mood {
        id: "noir",
        words: &[
            "noir",
            "detective",
            "monochrome",
            "black",
            "shadow",
            "grim",
            "horror",
        ],
        background: "#0a0a0b",
        ink: "#ededed",
        accent: "#b0b0b0",
        motion: SplashMotion::Fade,
        backdrop: SplashBackdrop::Vignette,
    },
    Mood {
        id: "royal",
        words: &[
            "royal", "fantasy", "kingdom", "medieval", "epic", "legend", "quest",
        ],
        background: "#150f2b",
        ink: "#fdf6e3",
        accent: "#e0b040",
        motion: SplashMotion::Rise,
        backdrop: SplashBackdrop::Band,
    },
    Mood {
        id: "space",
        words: &[
            "space", "galaxy", "cosmic", "star", "sci-fi", "scifi", "nebula", "orbit",
        ],
        background: "#050a1c",
        ink: "#eaf2ff",
        accent: "#7c5cff",
        motion: SplashMotion::Zoom,
        backdrop: SplashBackdrop::Vignette,
    },
];

/// Turn what the user wrote into a complete, valid spec.
///
/// Deterministic and offline by design. The brief steers the palette, the motion and the
/// hold; anything it does not mention takes a legible default. The result is always
/// [`validate`]-clean, so the panel can preview it the moment the user stops typing.
#[must_use]
pub fn synthesize(
    brief: &str,
    title: &str,
    tagline: &str,
    logo_res_path: Option<&str>,
) -> SplashSpec {
    let lowered = brief.to_ascii_lowercase();
    let words: Vec<&str> = lowered
        .split(|c: char| !c.is_ascii_alphanumeric() && c != '-')
        .filter(|word| !word.is_empty())
        .collect();
    let says = |needle: &str| words.contains(&needle);

    let mood = MOODS
        .iter()
        .find(|mood| mood.words.iter().any(|word| says(word)));

    let mut palette = mood.map_or_else(SplashPalette::default, |mood| SplashPalette {
        background: mood.background.to_owned(),
        ink: mood.ink.to_owned(),
        accent: mood.accent.to_owned(),
    });
    let mut motion = mood.map_or(SplashMotion::Fade, |mood| mood.motion);
    let mut backdrop = mood.map_or(SplashBackdrop::Solid, |mood| mood.backdrop);

    // An explicit colour in the brief outranks the mood it was inferred from.
    if let Some(hex) = first_hex(brief) {
        palette.accent = hex;
    }

    // An explicit motion or backdrop outranks it too.
    for candidate in SplashMotion::all() {
        if says(candidate.id()) {
            motion = *candidate;
        }
    }
    if says("static") || says("still") || says("no") && says("animation") {
        motion = SplashMotion::Still;
    }
    for candidate in SplashBackdrop::all() {
        if says(candidate.id()) {
            backdrop = *candidate;
        }
    }

    let duration_ms = duration_from(&lowered).unwrap_or(DEFAULT_SPLASH_MS);

    // A brief that asks for something big gets something big, within what stays on screen.
    let title_font_size = if says("huge") || says("bold") || says("massive") {
        96
    } else if says("small") || says("subtle") || says("understated") {
        56
    } else {
        72
    };

    SplashSpec {
        brief: clamp_chars(brief.trim(), MAX_BRIEF_CHARS),
        title: clamp_chars(title.trim(), MAX_TITLE_CHARS),
        tagline: clamp_chars(tagline.trim(), MAX_TAGLINE_CHARS),
        palette,
        motion,
        backdrop,
        duration_ms,
        logo_res_path: logo_res_path
            .map(str::trim)
            .filter(|path| !path.is_empty())
            .map(str::to_owned),
        title_font_size,
        tagline_font_size: 24,
        mood: mood.map_or("custom", |mood| mood.id).to_owned(),
    }
}

/// A hold the brief asked for in words, in milliseconds, when it named one in range.
fn duration_from(lowered: &str) -> Option<u32> {
    let bytes: Vec<char> = lowered.chars().collect();
    for (index, window) in bytes.windows(1).enumerate() {
        if !window[0].is_ascii_digit() {
            continue;
        }
        if index > 0 && bytes[index - 1].is_ascii_digit() {
            continue;
        }
        let mut digits = String::new();
        let mut cursor = index;
        while cursor < bytes.len() && bytes[cursor].is_ascii_digit() {
            digits.push(bytes[cursor]);
            cursor += 1;
        }
        // Only read it as a hold when the word after it says it is one.
        let rest: String = bytes[cursor..].iter().take(12).collect();
        let rest = rest.trim_start();
        if !(rest.starts_with('s')
            || rest.starts_with("sec")
            || rest.starts_with("second")
            || rest.starts_with("-second"))
        {
            continue;
        }
        if let Ok(seconds) = digits.parse::<u32>() {
            let millis = seconds.saturating_mul(1_000);
            if (MIN_SPLASH_MS..=MAX_SPLASH_MS).contains(&millis) {
                return Some(millis);
            }
        }
    }
    None
}

/// The first `#rrggbb` in the brief, normalised.
fn first_hex(brief: &str) -> Option<String> {
    let chars: Vec<char> = brief.chars().collect();
    for (index, ch) in chars.iter().enumerate() {
        if *ch != '#' {
            continue;
        }
        let hex: String = chars
            .iter()
            .skip(index + 1)
            .take(6)
            .filter(|c| c.is_ascii_hexdigit())
            .collect();
        if hex.len() == 6 {
            return Some(format!("#{}", hex.to_ascii_lowercase()));
        }
    }
    None
}

fn clamp_chars(value: &str, max: usize) -> String {
    value.chars().take(max).collect()
}

/// Parse `#rrggbb` into linear-ish 0…1 channels for a `.tscn` `Color`.
fn parse_hex(hex: &str) -> Result<(f64, f64, f64)> {
    let cleaned = hex.trim().trim_start_matches('#');
    if cleaned.len() != 6 || !cleaned.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(EngineError::Gate(
            format!("`{hex}` is not a colour"),
            Some("Use six hex digits, like #101820.".to_owned()),
        ));
    }
    let channel = |from: usize| -> f64 {
        u8::from_str_radix(&cleaned[from..from + 2], 16).unwrap_or(0) as f64 / 255.0
    };
    Ok((channel(0), channel(2), channel(4)))
}

/// Relative luminance, for the contrast gate.
fn luminance(hex: &str) -> Result<f64> {
    let (r, g, b) = parse_hex(hex)?;
    let linear = |channel: f64| {
        if channel <= 0.039_28 {
            channel / 12.92
        } else {
            ((channel + 0.055) / 1.055).powf(2.4)
        }
    };
    Ok(0.2126 * linear(r) + 0.7152 * linear(g) + 0.0722 * linear(b))
}

/// The WCAG contrast ratio between two colours.
fn contrast(one: &str, two: &str) -> Result<f64> {
    let a = luminance(one)?;
    let b = luminance(two)?;
    let (lighter, darker) = if a >= b { (a, b) } else { (b, a) };
    Ok((lighter + 0.05) / (darker + 0.05))
}

/// Every rule a splash must satisfy before it can be built.
///
/// These are gates, not warnings: a splash that fails one is refused rather than written and
/// apologised for afterwards. A splash is the first thing anybody sees of a game, and it is
/// the screen its author looks at least often once it works.
pub fn validate(spec: &SplashSpec) -> Result<()> {
    // A splash carries a name, a mark, or both. What it may not be is empty: a card with
    // nothing on it is three seconds of a coloured rectangle before the game starts.
    if !spec.has_content() {
        return Err(EngineError::Gate(
            "a splash needs a title or a logo".to_owned(),
            Some("Type a title, or upload a logo and leave the text blank.".to_owned()),
        ));
    }
    if spec.title.chars().count() > MAX_TITLE_CHARS {
        return Err(EngineError::Gate(
            format!("the title is longer than {MAX_TITLE_CHARS} characters"),
            Some("A splash letters a name, not a sentence.".to_owned()),
        ));
    }
    if spec.tagline.chars().count() > MAX_TAGLINE_CHARS {
        return Err(EngineError::Gate(
            format!("the tagline is longer than {MAX_TAGLINE_CHARS} characters"),
            None,
        ));
    }
    if !(MIN_SPLASH_MS..=MAX_SPLASH_MS).contains(&spec.duration_ms) {
        return Err(EngineError::Gate(
            format!(
                "a splash holds for {}ms, which is outside {MIN_SPLASH_MS}–{MAX_SPLASH_MS}ms",
                spec.duration_ms
            ),
            Some(
                "Shorter reads as a flicker; longer is something the player waits through."
                    .to_owned(),
            ),
        ));
    }
    // Legibility is only a question where there is something to read. A logo-only splash is
    // judged on its backdrop and its accent, not on a font size nothing uses.
    if spec.has_text() {
        if spec.title_font_size < MIN_TITLE_FONT {
            return Err(EngineError::Gate(
                format!(
                    "the title is set at {}px, under {MIN_TITLE_FONT}px",
                    spec.title_font_size
                ),
                None,
            ));
        }
        if spec.tagline_font_size < MIN_TAGLINE_FONT {
            return Err(EngineError::Gate(
                format!(
                    "the tagline is set at {}px, under {MIN_TAGLINE_FONT}px",
                    spec.tagline_font_size
                ),
                None,
            ));
        }

        let ratio = contrast(&spec.palette.ink, &spec.palette.background)?;
        if ratio < MIN_CONTRAST {
            return Err(EngineError::Gate(
                format!(
                    "the lettering contrasts {ratio:.1}:1 against the backdrop, under \
                     {MIN_CONTRAST:.1}:1"
                ),
                Some("Lighten the text or darken the background until it is readable.".to_owned()),
            ));
        }
    }
    // The accent draws the band and the rule under the title, so it has to be visible too.
    let accent_ratio = contrast(&spec.palette.accent, &spec.palette.background)?;
    if accent_ratio < 1.5 {
        return Err(EngineError::Gate(
            format!("the accent is invisible against the backdrop ({accent_ratio:.1}:1)"),
            Some("Pick an accent further from the background colour.".to_owned()),
        ));
    }

    if let Some(logo) = &spec.logo_res_path {
        if !logo.starts_with(super::RES_PREFIX) {
            return Err(EngineError::Gate(
                format!("the logo `{logo}` is not a project resource"),
                Some("Upload the logo so it is copied into the game.".to_owned()),
            ));
        }
    }
    Ok(())
}

/// What [`build`] needs beyond the spec itself.
#[derive(Clone, Debug)]
pub struct SplashBuildOptions {
    pub spec: SplashSpec,
    /// Where the splash scene goes, project-relative.
    pub scene_rel: String,
    /// Where the timing script goes, project-relative.
    pub script_rel: String,
    /// The scene the splash hands over to — the game's real first screen, as a `res://`
    /// path. This is what the project's main scene was before the splash took its place.
    pub next_scene_res: String,
    /// The project already has a splash scene, so the batch deletes it before writing the
    /// new one. Regenerating is the common case.
    pub replace_scene: bool,
}

impl SplashBuildOptions {
    /// The conventional build: the standard paths, handing over to `next_scene_res`.
    #[must_use]
    pub fn for_project(root: &Path, spec: SplashSpec, next_scene_res: &str) -> Self {
        Self {
            spec,
            scene_rel: SPLASH_SCENE_REL.to_owned(),
            script_rel: SPLASH_SCRIPT_REL.to_owned(),
            next_scene_res: next_scene_res.to_owned(),
            replace_scene: root.join(SPLASH_SCENE_REL).is_file(),
        }
    }
}

/// What a build produced.
#[derive(Clone, Debug)]
pub struct SplashBuild {
    pub batch: GodotActionBatch,
    /// The files the batch writes, project-relative.
    pub files: Vec<String>,
    /// The scene the splash hands over to.
    pub next_scene_res: String,
}

fn pf(name: &str, value: f64) -> (String, TscnValue) {
    (name.to_owned(), TscnValue::Float(value))
}

fn pi(name: &str, value: i64) -> (String, TscnValue) {
    (name.to_owned(), TscnValue::Int(value))
}

fn ps(name: &str, value: &str) -> (String, TscnValue) {
    (name.to_owned(), TscnValue::Str(value.to_owned()))
}

fn pcolor(name: &str, hex: &str) -> Result<(String, TscnValue)> {
    let (r, g, b) = parse_hex(hex)?;
    Ok((name.to_owned(), TscnValue::Color(r, g, b, 1.0)))
}

/// `MOUSE_FILTER_IGNORE`: nothing on a splash is clickable.
const MOUSE_IGNORE: i64 = 2;
/// `PRESET_FULL_RECT`-equivalent anchors are set by hand; this is `SIZE_SHRINK_CENTER`.
const SIZE_SHRINK_CENTER: i64 = 4;
/// `HORIZONTAL_ALIGNMENT_CENTER` / `VERTICAL_ALIGNMENT_CENTER`.
const ALIGN_CENTER: i64 = 1;

/// Lower a spec into the typed batch that builds it.
///
/// The batch ends with [`GodotAction::SetMainScene`], which is what makes the splash play at
/// every start of this game rather than being a scene nobody opens.
pub fn build(options: &SplashBuildOptions) -> Result<SplashBuild> {
    validate(&options.spec)?;

    let splash_res = format!("{}{}", super::RES_PREFIX, options.scene_rel);
    let next = options.next_scene_res.trim();
    if next.is_empty() {
        return Err(EngineError::Gate(
            "the splash has nowhere to hand over to".to_owned(),
            Some("Set the game's main scene first; the splash plays in front of it.".to_owned()),
        ));
    }
    // The gate that matters most in this file. A splash whose next scene is itself boots
    // into a loop the player cannot leave, and the game never starts.
    if next == splash_res {
        return Err(EngineError::Gate(
            "the splash would hand over to itself".to_owned(),
            Some(
                "Point the game's main scene at the first real screen; the splash is put in \
                 front of it automatically."
                    .to_owned(),
            ),
        ));
    }

    let spec = &options.spec;
    let mut actions: Vec<GodotAction> = Vec::new();

    if options.replace_scene {
        actions.push(GodotAction::DeleteScene {
            path: options.scene_rel.clone(),
        });
    }

    actions.push(GodotAction::CreateScene {
        path: options.scene_rel.clone(),
        root_name: SPLASH_NODE_NAME.to_owned(),
        root_type: "CanvasLayer".to_owned(),
    });
    actions.push(GodotAction::SetProperty {
        scene: options.scene_rel.clone(),
        path: ".".to_owned(),
        property: "layer".to_owned(),
        value: TscnValue::Int(SPLASH_CANVAS_LAYER),
    });

    let scene = options.scene_rel.clone();
    let mut node = |parent: &str, name: &str, type_: &str, properties: Vec<(String, TscnValue)>| {
        actions.push(GodotAction::AddNode {
            scene: scene.clone(),
            parent: parent.to_owned(),
            name: name.to_owned(),
            type_: type_.to_owned(),
            properties,
            groups: Vec::new(),
        });
        if parent == "." {
            name.to_owned()
        } else {
            format!("{parent}/{name}")
        }
    };

    // The whole-viewport backdrop. Everything else is centred inside it.
    let backdrop = node(
        ".",
        "Backdrop",
        "ColorRect",
        vec![
            pf("anchor_right", 1.0),
            pf("anchor_bottom", 1.0),
            pi("mouse_filter", MOUSE_IGNORE),
            pcolor("color", &spec.palette.background)?,
        ],
    );

    if spec.backdrop == SplashBackdrop::Vignette {
        // A darkened wash under the lettering. `scale` is a Vector2 on a Control, so the
        // float this used to set was a type Godot refuses — the whole scene failed to load
        // rather than the one property being ignored.
        node(
            &backdrop,
            "Vignette",
            "ColorRect",
            vec![
                pf("anchor_right", 1.0),
                pf("anchor_bottom", 1.0),
                pi("mouse_filter", MOUSE_IGNORE),
                ("color".to_owned(), TscnValue::Color(0.0, 0.0, 0.0, 0.35)),
            ],
        );
    }

    let centre = node(
        &backdrop,
        "Centre",
        "CenterContainer",
        vec![
            pf("anchor_right", 1.0),
            pf("anchor_bottom", 1.0),
            pi("mouse_filter", MOUSE_IGNORE),
        ],
    );

    let stack = node(
        &centre,
        "Stack",
        "VBoxContainer",
        vec![
            pi("mouse_filter", MOUSE_IGNORE),
            pi("alignment", ALIGN_CENTER),
            pi("theme_override_constants/separation", 18),
        ],
    );

    if spec.backdrop == SplashBackdrop::Band {
        node(
            &stack,
            "Band",
            "ColorRect",
            vec![
                pcolor("color", &spec.palette.accent)?,
                pi("mouse_filter", MOUSE_IGNORE),
                pi("size_flags_horizontal", SIZE_SHRINK_CENTER),
                (
                    "custom_minimum_size".to_owned(),
                    TscnValue::Vector2(420.0, 6.0),
                ),
            ],
        );
    }

    if let Some(logo) = &spec.logo_res_path {
        node(
            &stack,
            "Logo",
            "TextureRect",
            vec![
                ("texture".to_owned(), TscnValue::ExtResource(logo.clone())),
                // `EXPAND_FIT_WIDTH_PROPORTIONAL` with `KEEP_ASPECT_CENTERED`, so a logo of
                // any shape lands centred and undistorted.
                pi("expand_mode", 3),
                pi("stretch_mode", 5),
                pi("size_flags_horizontal", SIZE_SHRINK_CENTER),
                (
                    "custom_minimum_size".to_owned(),
                    TscnValue::Vector2(320.0, 200.0),
                ),
                pi("mouse_filter", MOUSE_IGNORE),
            ],
        );
    }

    // A logo-only splash draws no empty `Label` and no rule under nothing: an empty label
    // still takes a line of height, which pushes the logo off centre for no visible reason.
    if !spec.title.trim().is_empty() {
        node(
            &stack,
            "Title",
            "Label",
            vec![
                ps("text", &spec.title),
                pi("horizontal_alignment", ALIGN_CENTER),
                pi("vertical_alignment", ALIGN_CENTER),
                pi("theme_override_font_sizes/font_size", spec.title_font_size),
                pcolor("theme_override_colors/font_color", &spec.palette.ink)?,
                pi("size_flags_horizontal", SIZE_SHRINK_CENTER),
                pi("mouse_filter", MOUSE_IGNORE),
            ],
        );
    }

    if spec.has_text() {
        node(
            &stack,
            "Rule",
            "ColorRect",
            vec![
                pcolor("color", &spec.palette.accent)?,
                (
                    "custom_minimum_size".to_owned(),
                    TscnValue::Vector2(160.0, 3.0),
                ),
                pi("size_flags_horizontal", SIZE_SHRINK_CENTER),
                pi("mouse_filter", MOUSE_IGNORE),
            ],
        );
    }

    if !spec.tagline.trim().is_empty() {
        node(
            &stack,
            "Tagline",
            "Label",
            vec![
                ps("text", &spec.tagline),
                pi("horizontal_alignment", ALIGN_CENTER),
                pi("vertical_alignment", ALIGN_CENTER),
                pi(
                    "theme_override_font_sizes/font_size",
                    spec.tagline_font_size,
                ),
                pcolor("theme_override_colors/font_color", &spec.palette.ink)?,
                pi("size_flags_horizontal", SIZE_SHRINK_CENTER),
                pi("mouse_filter", MOUSE_IGNORE),
            ],
        );
    }

    actions.push(GodotAction::WriteScript {
        path: options.script_rel.clone(),
        source: splash_script(spec, next),
    });
    actions.push(GodotAction::AttachScript {
        scene: options.scene_rel.clone(),
        path: ".".to_owned(),
        script_res_path: format!("{}{}", super::RES_PREFIX, options.script_rel),
    });
    // What makes it a splash rather than a scene: the game now boots into it.
    actions.push(GodotAction::SetMainScene {
        res_path: splash_res,
    });

    Ok(SplashBuild {
        batch: GodotActionBatch::new(format!("splash screen — {}", spec.title), actions),
        files: vec![options.scene_rel.clone(), options.script_rel.clone()],
        next_scene_res: next.to_owned(),
    })
}

/// The GDScript that times the splash and hands over.
///
/// Written here rather than by the model, and check-compiled by the apply path before it is
/// allowed to stay on disk (INV-088). It fades in, holds, fades out and changes scene; any
/// input skips straight to the handover, because a splash the player cannot skip is the
/// first thing they will resent about the game.
fn splash_script(spec: &SplashSpec, next_scene_res: &str) -> String {
    let hold = f64::from(spec.duration_ms) / 1000.0;
    // The fades live inside the hold, so the total is what the panel promised.
    let fade = if spec.motion == SplashMotion::Still {
        0.0
    } else {
        (hold * 0.22).min(0.8)
    };
    let middle = (hold - fade * 2.0).max(0.2);
    let rise = spec.motion == SplashMotion::Rise;
    let zoom = spec.motion == SplashMotion::Zoom;

    let mut source = String::new();
    source.push_str("extends CanvasLayer\n");
    source.push_str(
        "## Generated by Bhippi (GAD-161). Rebuilt whenever the splash is regenerated.\n\n",
    );
    source.push_str(&format!("const NEXT_SCENE := \"{next_scene_res}\"\n"));
    source.push_str(&format!("const FADE_SECONDS := {fade:.3}\n"));
    source.push_str(&format!("const HOLD_SECONDS := {middle:.3}\n\n"));
    source.push_str("var _handed_over := false\n\n");
    source.push_str("func _ready() -> void:\n");
    source.push_str("\tvar stack := get_node_or_null(\"Backdrop/Centre/Stack\") as Control\n");
    source.push_str("\tif stack == null:\n");
    source.push_str("\t\t_hand_over()\n");
    source.push_str("\t\treturn\n");
    if fade > 0.0 {
        source.push_str("\tstack.modulate.a = 0.0\n");
    }
    if rise {
        source.push_str("\tstack.position.y += 28.0\n");
    }
    if zoom {
        source.push_str("\tstack.pivot_offset = stack.size * 0.5\n");
        source.push_str("\tstack.scale = Vector2(0.92, 0.92)\n");
    }
    source.push_str("\tvar tween := create_tween()\n");
    source.push_str("\ttween.set_parallel(true)\n");
    if fade > 0.0 {
        source.push_str(
            "\ttween.tween_property(stack, \"modulate:a\", 1.0, FADE_SECONDS)\\\n\t\t.set_trans(Tween.TRANS_SINE)\n",
        );
    }
    if rise {
        source.push_str(
            "\ttween.tween_property(stack, \"position:y\", stack.position.y - 28.0, FADE_SECONDS)\\\n\t\t.set_trans(Tween.TRANS_SINE)\n",
        );
    }
    if zoom {
        source.push_str(
            "\ttween.tween_property(stack, \"scale\", Vector2.ONE, FADE_SECONDS)\\\n\t\t.set_trans(Tween.TRANS_SINE)\n",
        );
    }
    source.push_str("\tawait tween.finished\n");
    source.push_str("\tawait get_tree().create_timer(HOLD_SECONDS).timeout\n");
    if fade > 0.0 {
        source.push_str("\tvar out_tween := create_tween()\n");
        source.push_str("\tout_tween.tween_property(stack, \"modulate:a\", 0.0, FADE_SECONDS)\n");
        source.push_str("\tawait out_tween.finished\n");
    }
    source.push_str("\t_hand_over()\n\n");
    source.push_str("func _unhandled_input(event: InputEvent) -> void:\n");
    source.push_str("\t## Any deliberate press skips the splash.\n");
    source.push_str("\tif event.is_pressed() and not event.is_echo():\n");
    source.push_str("\t\t_hand_over()\n\n");
    source.push_str("func _hand_over() -> void:\n");
    source.push_str("\tif _handed_over:\n");
    source.push_str("\t\treturn\n");
    source.push_str("\t_handed_over = true\n");
    source.push_str("\tif ResourceLoader.exists(NEXT_SCENE):\n");
    source.push_str("\t\tget_tree().change_scene_to_file(NEXT_SCENE)\n");
    source.push_str("\telse:\n");
    source.push_str("\t\tpush_error(\"splash: next scene is missing: %s\" % NEXT_SCENE)\n");
    source
}

/// The splash as a standalone SVG, for export.
///
/// Deterministic and engine-free: an export that needed a working Godot install would fail
/// exactly when someone wanted the file for a store page or a press kit. The logo is named
/// but not embedded — it is exported beside this as its own file.
#[must_use]
pub fn svg(spec: &SplashSpec) -> String {
    let width = super::scaffold::VIEWPORT_WIDTH;
    let height = super::scaffold::VIEWPORT_HEIGHT;
    let centre_x = width / 2;
    let escape = |value: &str| {
        value
            .replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
    };

    let mut out = String::new();
    out.push_str(&format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{width}\" height=\"{height}\" \
         viewBox=\"0 0 {width} {height}\" role=\"img\" aria-label=\"{}\">\n",
        if spec.title.trim().is_empty() {
            "Splash screen".to_owned()
        } else {
            format!("{} splash screen", escape(&spec.title))
        }
    ));
    out.push_str(&format!(
        "  <rect width=\"{width}\" height=\"{height}\" fill=\"{}\"/>\n",
        spec.palette.background
    ));
    if spec.backdrop == SplashBackdrop::Vignette {
        out.push_str("  <defs><radialGradient id=\"v\" cx=\"50%\" cy=\"50%\" r=\"75%\">\n");
        out.push_str("    <stop offset=\"55%\" stop-color=\"#000\" stop-opacity=\"0\"/>\n");
        out.push_str("    <stop offset=\"100%\" stop-color=\"#000\" stop-opacity=\"0.45\"/>\n");
        out.push_str("  </radialGradient></defs>\n");
        out.push_str(&format!(
            "  <rect width=\"{width}\" height=\"{height}\" fill=\"url(#v)\"/>\n"
        ));
    }

    let mut cursor = height / 2 - 40;
    if spec.logo_res_path.is_some() {
        // The logo ships beside the SVG; this is its reserved frame, so a designer opening
        // the file sees where it belongs rather than a silently different layout.
        out.push_str(&format!(
            "  <rect x=\"{}\" y=\"{}\" width=\"320\" height=\"200\" rx=\"12\" fill=\"none\" \
             stroke=\"{}\" stroke-opacity=\"0.35\" stroke-dasharray=\"8 6\"/>\n",
            centre_x - 160,
            cursor - 230,
            spec.palette.accent
        ));
        out.push_str(&format!(
            "  <text x=\"{centre_x}\" y=\"{}\" text-anchor=\"middle\" font-family=\"sans-serif\" \
             font-size=\"16\" fill=\"{}\" fill-opacity=\"0.55\">logo.png</text>\n",
            cursor - 120,
            spec.palette.ink
        ));
    }
    if spec.backdrop == SplashBackdrop::Band {
        out.push_str(&format!(
            "  <rect x=\"{}\" y=\"{}\" width=\"420\" height=\"6\" fill=\"{}\"/>\n",
            centre_x - 210,
            cursor - 70,
            spec.palette.accent
        ));
    }

    // The export mirrors the scene: a logo-only splash letters nothing, so it gets no empty
    // text node and no rule drawn under nothing.
    if !spec.title.trim().is_empty() {
        out.push_str(&format!(
            "  <text x=\"{centre_x}\" y=\"{cursor}\" text-anchor=\"middle\" \
             font-family=\"Segoe UI, Helvetica, Arial, sans-serif\" font-weight=\"700\" \
             font-size=\"{}\" fill=\"{}\">{}</text>\n",
            spec.title_font_size,
            spec.palette.ink,
            escape(&spec.title)
        ));
    }
    if spec.has_text() {
        cursor += 34;
        out.push_str(&format!(
            "  <rect x=\"{}\" y=\"{cursor}\" width=\"160\" height=\"3\" fill=\"{}\"/>\n",
            centre_x - 80,
            spec.palette.accent
        ));
    }
    if !spec.tagline.trim().is_empty() {
        cursor += 44;
        out.push_str(&format!(
            "  <text x=\"{centre_x}\" y=\"{cursor}\" text-anchor=\"middle\" \
             font-family=\"Segoe UI, Helvetica, Arial, sans-serif\" font-size=\"{}\" \
             fill=\"{}\" fill-opacity=\"0.85\">{}</text>\n",
            spec.tagline_font_size,
            spec.palette.ink,
            escape(&spec.tagline)
        ));
    }
    out.push_str("</svg>\n");
    out
}

/// What the panel needs to know about the splash this project already has.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize, Type)]
pub struct SplashProjectState {
    /// True when the generated scene is on disk.
    pub installed: bool,
    /// True when the project actually boots into it.
    pub is_main_scene: bool,
    /// The spec last built here, when one was recorded.
    pub spec: Option<SplashSpec>,
    /// The scene the splash hands over to, or the project's main scene when no splash is
    /// installed yet. Empty when the project has no main scene at all.
    pub next_scene_res: String,
}

/// Read what this project's splash currently is.
///
/// Never fails: a project with no splash, no manifest or no main scene is a legitimate
/// starting state, and the panel needs to draw something for it.
#[must_use]
pub fn project_state(project_root: &Path) -> SplashProjectState {
    let scene_path = project_root.join(SPLASH_SCENE_REL);
    let installed = scene_path.is_file();
    let spec = std::fs::read_to_string(project_root.join(SPLASH_SPEC_REL))
        .ok()
        .and_then(|text| serde_json::from_str::<SplashSpec>(&text).ok());

    let main_scene = main_scene_of(project_root).unwrap_or_default();
    let splash_res = format!("{}{}", super::RES_PREFIX, SPLASH_SCENE_REL);
    let is_main_scene = main_scene == splash_res;

    // When the splash is already the main scene, the handover target is what the script was
    // built to go to, not `project.godot` — which now points at the splash itself.
    let next_scene_res = if is_main_scene {
        recorded_next_scene(project_root).unwrap_or_default()
    } else {
        main_scene
    };

    SplashProjectState {
        installed,
        is_main_scene,
        spec,
        next_scene_res,
    }
}

/// `[application] run/main_scene` from this project, when it has one.
fn main_scene_of(project_root: &Path) -> Option<String> {
    let text = std::fs::read_to_string(project_root.join(super::action::PROJECT_FILE)).ok()?;
    super::project::GodotProjectFile::parse(&text)
        .ok()
        .and_then(|file| file.main_scene())
}

/// The handover target recorded in the generated script.
///
/// Read back from the script rather than kept in a second place, so the two can never
/// disagree about where the game actually goes after the splash.
fn recorded_next_scene(project_root: &Path) -> Option<String> {
    let source = std::fs::read_to_string(project_root.join(SPLASH_SCRIPT_REL)).ok()?;
    let line = source
        .lines()
        .find(|line| line.trim_start().starts_with("const NEXT_SCENE"))?;
    let start = line.find('"')?;
    let rest = &line[start + 1..];
    let end = rest.find('"')?;
    Some(rest[..end].to_owned())
}

/// The motion and backdrop choices, for the panel's pickers.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct SplashChoice {
    pub id: String,
    pub title: String,
}

/// Everything the panel needs to draw its pickers, decided here (INV-051).
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct SplashLibraryView {
    pub motions: Vec<SplashChoice>,
    pub backdrops: Vec<SplashChoice>,
    pub min_duration_ms: u32,
    pub max_duration_ms: u32,
    pub max_brief_chars: usize,
    pub max_title_chars: usize,
    pub max_tagline_chars: usize,
}

/// The library the panel opens on.
#[must_use]
pub fn library() -> SplashLibraryView {
    SplashLibraryView {
        motions: SplashMotion::all()
            .iter()
            .map(|entry| SplashChoice {
                id: entry.id().to_owned(),
                title: entry.title().to_owned(),
            })
            .collect(),
        backdrops: SplashBackdrop::all()
            .iter()
            .map(|entry| SplashChoice {
                id: entry.id().to_owned(),
                title: entry.title().to_owned(),
            })
            .collect(),
        min_duration_ms: MIN_SPLASH_MS,
        max_duration_ms: MAX_SPLASH_MS,
        max_brief_chars: MAX_BRIEF_CHARS,
        max_title_chars: MAX_TITLE_CHARS,
        max_tagline_chars: MAX_TAGLINE_CHARS,
    }
}

/// Resolve a motion id the panel sent.
#[must_use]
pub fn motion(id: &str) -> Option<SplashMotion> {
    SplashMotion::from_id(id)
}

/// Resolve a backdrop id the panel sent.
#[must_use]
pub fn backdrop(id: &str) -> Option<SplashBackdrop> {
    SplashBackdrop::from_id(id)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec() -> SplashSpec {
        synthesize("a calm forest game", "Greenwood", "the long way home", None)
    }

    /// A throwaway Godot project, so a build can be written to disk and read back.
    fn scratch_project(name: &str) -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!("bhippi-splash-{name}-{}", ulid::Ulid::new()));
        std::fs::create_dir_all(root.join("scenes")).expect("scenes dir");
        std::fs::create_dir_all(root.join("scripts")).expect("scripts dir");
        std::fs::create_dir_all(root.join("assets/ui/splash")).expect("logo dir");
        std::fs::write(
            root.join("scenes/main.tscn"),
            "[gd_scene load_steps=1 format=3]\n\n[node name=\"Main\" type=\"Node2D\"]\n",
        )
        .expect("main scene");
        std::fs::write(
            root.join("project.godot"),
            "config_version=5\n\n[application]\n\nconfig/name=\"Demo\"\n\
             run/main_scene=\"res://scenes/main.tscn\"\n",
        )
        .expect("project file");
        std::fs::write(root.join("assets/ui/splash/logo.png"), [0u8; 8]).expect("logo");
        root
    }

    /// Write a built splash to disk and hand back the scene text.
    fn apply(root: &std::path::Path, built: &SplashBuild) -> String {
        let changeset = super::super::action::lower(root, &built.batch)
            .unwrap_or_else(|error| panic!("the batch must lower: {error}"));
        super::super::action::apply_changeset(root, &changeset)
            .unwrap_or_else(|error| panic!("the changeset must apply: {error}"));
        std::fs::read_to_string(root.join(SPLASH_SCENE_REL)).expect("the splash scene")
    }

    fn options(spec: SplashSpec) -> SplashBuildOptions {
        SplashBuildOptions {
            spec,
            scene_rel: SPLASH_SCENE_REL.to_owned(),
            script_rel: SPLASH_SCRIPT_REL.to_owned(),
            next_scene_res: "res://scenes/main.tscn".to_owned(),
            replace_scene: false,
        }
    }

    /// The defect the owner hit: the logo never appeared.
    ///
    /// `ExtResource("…")` in a `.tscn` holds an **id** that the file's header has to declare.
    /// The build named the logo by its `res://` path instead, so the scene referenced an id
    /// nothing declared and Godot loaded the texture as null — no error anywhere, just an
    /// empty splash.
    #[test]
    fn the_logo_is_declared_as_a_resource_the_scene_can_actually_load() {
        let root = scratch_project("logo");
        let mut spec = spec();
        spec.logo_res_path = Some("res://assets/ui/splash/logo.png".to_owned());
        let built = build(&options(spec)).expect("the splash must build");
        let scene = apply(&root, &built);

        assert!(
            scene.contains(
                "[ext_resource type=\"Texture2D\" path=\"res://assets/ui/splash/logo.png\""
            ),
            "the logo must be declared in the scene header:\n{scene}"
        );

        // And the node must point at that declared id, not at the path.
        let document = super::super::tscn::parse(&scene).expect("the scene must parse");
        let id = document
            .ext_resource_by_path("res://assets/ui/splash/logo.png")
            .map(|resource| resource.id.clone())
            .expect("the logo resource must exist");
        assert!(
            scene.contains(&format!("texture = ExtResource(\"{id}\")")),
            "the TextureRect must reference the declared id, not the path:\n{scene}"
        );
        assert!(
            !scene.contains("ExtResource(\"res://"),
            "no property may still name a resource by path:\n{scene}"
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    /// The owner asked for this outright: a splash may be the logo alone.
    #[test]
    fn a_splash_can_be_the_logo_alone_with_no_lettering() {
        let root = scratch_project("logo-only");
        let spec = SplashSpec {
            title: String::new(),
            tagline: String::new(),
            logo_res_path: Some("res://assets/ui/splash/logo.png".to_owned()),
            ..synthesize("just the logo, nothing else", "", "", None)
        };
        validate(&spec).expect("a logo with no words is a splash");

        let built = build(&options(spec)).expect("the splash must build");
        let scene = apply(&root, &built);

        assert!(scene.contains("type=\"TextureRect\""), "the logo is drawn");
        assert!(
            !scene.contains("type=\"Label\""),
            "an empty label still takes a line of height and pushes the logo off centre:\n{scene}"
        );
        assert!(
            !scene.contains("name=\"Rule\""),
            "no rule is drawn under nothing:\n{scene}"
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    /// Neither words nor a mark is not a splash, it is a coloured rectangle.
    #[test]
    fn a_splash_with_neither_text_nor_logo_is_refused() {
        let spec = SplashSpec {
            title: String::new(),
            tagline: String::new(),
            logo_res_path: None,
            ..SplashSpec::default()
        };
        let error = validate(&spec).expect_err("an empty card must be refused");
        assert!(
            format!("{error}").contains("title or a logo"),
            "got {error}"
        );
    }

    /// The whole point: after a build the game opens on the splash, and the splash is a
    /// scene Godot can actually load.
    #[test]
    fn the_built_project_boots_into_a_scene_that_parses() {
        let root = scratch_project("boot");
        let built = build(&options(spec())).expect("the splash must build");
        let scene = apply(&root, &built);

        super::super::tscn::parse(&scene).expect("the splash scene must parse");

        let project = std::fs::read_to_string(root.join("project.godot")).expect("project file");
        assert!(
            project.contains("run/main_scene=\"res://scenes/splash.tscn\""),
            "the game must boot into the splash:\n{project}"
        );

        let script = std::fs::read_to_string(root.join(SPLASH_SCRIPT_REL)).expect("the script");
        assert!(
            script.contains("res://scenes/main.tscn"),
            "and the splash must hand over to what the game booted into before"
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_brief_steers_the_palette_the_motion_and_the_hold() {
        let neon = synthesize(
            "neon cyberpunk arcade, hold it for 4 seconds",
            "Rez",
            "",
            None,
        );
        assert_eq!(neon.palette.background, "#12002e");
        assert_eq!(neon.motion, SplashMotion::Zoom);
        assert_eq!(neon.duration_ms, 4_000, "the brief named a hold in range");

        let paper = synthesize("clean minimal flat", "Paper", "", None);
        assert_eq!(paper.palette.background, "#f4f1ea");
        assert_eq!(paper.motion, SplashMotion::Fade);
        assert_eq!(
            paper.duration_ms, DEFAULT_SPLASH_MS,
            "a brief with no hold takes the default"
        );
    }

    #[test]
    fn a_hold_outside_the_allowed_range_is_ignored_rather_than_clamped_silently() {
        // 30 seconds is not a splash. Taking the default is honest; clamping to 5 would
        // silently pretend the brief was reasonable.
        let long = synthesize("hold for 30 seconds", "Slow", "", None);
        assert_eq!(long.duration_ms, DEFAULT_SPLASH_MS);
        let short = synthesize("hold for 1 second", "Fast", "", None);
        assert_eq!(short.duration_ms, DEFAULT_SPLASH_MS);
    }

    #[test]
    fn an_explicit_colour_in_the_brief_outranks_the_mood() {
        let spec = synthesize("neon arcade but accent #00ff9c", "Rez", "", None);
        assert_eq!(spec.palette.accent, "#00ff9c");
        assert_eq!(
            spec.palette.background, "#12002e",
            "the mood still sets the rest"
        );
    }

    #[test]
    fn everything_synthesised_is_valid_without_further_editing() {
        for brief in [
            "",
            "neon cyberpunk",
            "clean minimal paper",
            "grim noir detective",
            "royal fantasy kingdom",
            "cosmic space station",
            "frozen arctic",
            "volcanic fire",
            "cozy forest",
        ] {
            let spec = synthesize(brief, "Title", "tagline", None);
            validate(&spec).unwrap_or_else(|error| {
                panic!("synthesised spec for {brief:?} must be valid: {error}")
            });
        }
    }

    #[test]
    fn unreadable_lettering_is_refused() {
        let mut spec = spec();
        spec.palette.ink = "#101010".to_owned();
        spec.palette.background = "#151515".to_owned();
        let error = validate(&spec).expect_err("a splash nobody can read must be refused");
        assert!(format!("{error}").contains("contrast"), "got {error}");
    }

    #[test]
    fn a_hold_outside_three_to_five_seconds_is_refused() {
        let mut spec = spec();
        spec.duration_ms = 900;
        assert!(validate(&spec).is_err(), "a flicker is not a splash");
        spec.duration_ms = 20_000;
        assert!(validate(&spec).is_err(), "nor is a wait");
        spec.duration_ms = MIN_SPLASH_MS;
        assert!(validate(&spec).is_ok());
        spec.duration_ms = MAX_SPLASH_MS;
        assert!(validate(&spec).is_ok());
    }

    #[test]
    fn a_splash_that_hands_over_to_itself_is_refused() {
        // The one that stops the game booting at all.
        let options = SplashBuildOptions {
            spec: spec(),
            scene_rel: SPLASH_SCENE_REL.to_owned(),
            script_rel: SPLASH_SCRIPT_REL.to_owned(),
            next_scene_res: format!("res://{SPLASH_SCENE_REL}"),
            replace_scene: false,
        };
        let error = build(&options).expect_err("a boot loop must be refused");
        assert!(format!("{error}").contains("itself"), "got {error}");
    }

    #[test]
    fn a_splash_with_nowhere_to_go_is_refused() {
        let options = SplashBuildOptions {
            spec: spec(),
            scene_rel: SPLASH_SCENE_REL.to_owned(),
            script_rel: SPLASH_SCRIPT_REL.to_owned(),
            next_scene_res: "   ".to_owned(),
            replace_scene: false,
        };
        assert!(build(&options).is_err());
    }

    #[test]
    fn the_build_makes_the_splash_the_scene_the_game_boots_into() {
        let options = SplashBuildOptions {
            spec: spec(),
            scene_rel: SPLASH_SCENE_REL.to_owned(),
            script_rel: SPLASH_SCRIPT_REL.to_owned(),
            next_scene_res: "res://scenes/main.tscn".to_owned(),
            replace_scene: false,
        };
        let built = build(&options).expect("the splash must build");

        let sets_main = built.batch.actions.iter().any(|action| {
            matches!(action, GodotAction::SetMainScene { res_path }
                if res_path == &format!("res://{SPLASH_SCENE_REL}"))
        });
        assert!(sets_main, "without this the splash never plays");

        let script = built
            .batch
            .actions
            .iter()
            .find_map(|action| match action {
                GodotAction::WriteScript { source, .. } => Some(source),
                _ => None,
            })
            .expect("a splash writes its timing script");
        assert!(
            script.contains("res://scenes/main.tscn"),
            "the script must carry the handover target"
        );
        assert!(
            script.contains("change_scene_to_file"),
            "the splash must actually hand over"
        );
        assert!(
            script.contains("_unhandled_input"),
            "a splash the player cannot skip is one they resent"
        );
        assert_eq!(built.files.len(), 2, "a scene and its script");
    }

    #[test]
    fn a_rebuild_deletes_the_scene_it_replaces() {
        let options = SplashBuildOptions {
            spec: spec(),
            scene_rel: SPLASH_SCENE_REL.to_owned(),
            script_rel: SPLASH_SCRIPT_REL.to_owned(),
            next_scene_res: "res://scenes/main.tscn".to_owned(),
            replace_scene: true,
        };
        let built = build(&options).expect("the splash must build");
        assert!(
            matches!(
                built.batch.actions.first(),
                Some(GodotAction::DeleteScene { .. })
            ),
            "a rebuild replaces rather than merges"
        );
    }

    #[test]
    fn the_fades_fit_inside_the_hold_the_panel_promised() {
        // Otherwise a "3 second" splash is on screen for four and a half.
        let mut base = spec();
        base.duration_ms = 3_000;
        let options = SplashBuildOptions {
            spec: base,
            scene_rel: SPLASH_SCENE_REL.to_owned(),
            script_rel: SPLASH_SCRIPT_REL.to_owned(),
            next_scene_res: "res://scenes/main.tscn".to_owned(),
            replace_scene: false,
        };
        let built = build(&options).expect("build");
        let script = built
            .batch
            .actions
            .iter()
            .find_map(|action| match action {
                GodotAction::WriteScript { source, .. } => Some(source.clone()),
                _ => None,
            })
            .expect("script");

        let read = |key: &str| -> f64 {
            script
                .lines()
                .find(|line| line.starts_with(key))
                .and_then(|line| line.split(":=").nth(1))
                .and_then(|value| value.trim().parse::<f64>().ok())
                .unwrap_or_else(|| panic!("{key} must be in the script"))
        };
        let fade = read("const FADE_SECONDS");
        let hold = read("const HOLD_SECONDS");
        assert!(
            (fade * 2.0 + hold - 3.0).abs() < 0.01,
            "fade in + hold + fade out must be the promised {fade} {hold}"
        );
    }

    #[test]
    fn the_svg_export_carries_the_lettering_and_the_colours() {
        let mut spec = spec();
        spec.title = "Green & Wood".to_owned();
        let drawing = svg(&spec);
        assert!(drawing.starts_with("<svg"), "it must be an SVG");
        assert!(drawing.contains(&spec.palette.background));
        assert!(
            drawing.contains("Green &amp; Wood"),
            "the title must be escaped, not injected"
        );
        assert!(drawing.trim_end().ends_with("</svg>"));
    }

    #[test]
    fn a_logo_only_splash_exports_no_empty_lettering() {
        let spec = SplashSpec {
            title: String::new(),
            tagline: String::new(),
            logo_res_path: Some("res://assets/ui/splash/logo.png".to_owned()),
            ..SplashSpec::default()
        };
        let drawing = svg(&spec);
        assert!(
            !drawing.contains("font-weight=\"700\""),
            "no title is lettered when there is no title:\n{drawing}"
        );
        assert!(
            !drawing.contains("width=\"160\" height=\"3\""),
            "and no rule is drawn under nothing:\n{drawing}"
        );
        assert!(
            drawing.contains("logo.png"),
            "the logo's frame is still reserved:\n{drawing}"
        );
        assert!(
            drawing.contains("aria-label=\"Splash screen\""),
            "an untitled splash still needs a name a screen reader can say:\n{drawing}"
        );
    }

    #[test]
    fn the_generated_script_is_indented_with_tabs_as_gdscript_requires() {
        let options = SplashBuildOptions {
            spec: spec(),
            scene_rel: SPLASH_SCENE_REL.to_owned(),
            script_rel: SPLASH_SCRIPT_REL.to_owned(),
            next_scene_res: "res://scenes/main.tscn".to_owned(),
            replace_scene: false,
        };
        let built = build(&options).expect("build");
        let script = built
            .batch
            .actions
            .iter()
            .find_map(|action| match action {
                GodotAction::WriteScript { source, .. } => Some(source.clone()),
                _ => None,
            })
            .expect("script");
        for line in script.lines() {
            let indent: String = line.chars().take_while(|c| c.is_whitespace()).collect();
            assert!(
                !indent.contains(' '),
                "GDScript bodies are tab-indented; found spaces in {line:?}"
            );
        }
    }
}
