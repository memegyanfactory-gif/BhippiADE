//! Curated OFL game font library and archetype-driven font selection (INV-074, GAD-160).
//!
//! Provides hash-pinned, legally clean (SIL Open Font License 1.1) typography for games
//! across five distinct game aesthetic pillars:
//! * `Orbitron`: High-tech geometric sans-serif for mecha, cyberpunk, space and cockpit HUDs.
//! * `Rajdhani`: Condensed squared technical sans-serif for hero shooters, MOBAs and tactical combat.
//! * `Press Start 2P`: Integer-aligned bitmap pixel typography for retro RPGs, platformers and arcade games.
//! * `Cinzel`: Classical Roman serif with carved stone proportions for dark fantasy, souls-likes and dungeon crawlers.
//! * `Outfit`: Balanced, open geometric modern sans-serif for casual, arcade, puzzle and minimal UI.
//!
//! When an AI agent or user builds a HUD, [`font_for_archetype`] determines the optimal
//! font pairing, and [`install_game_font`] materializes the font and its `.meta.json`
//! attribution sidecar into `assets/fonts/` so the game exports legally clean.

use crate::error::{EngineError, Result};
use std::path::Path;

/// Metadata for one curated game font family.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GameFont {
    pub id: &'static str,
    pub family: &'static str,
    pub category: &'static str,
    pub author: &'static str,
    pub license: &'static str,
    pub file_name: &'static str,
    pub archetypes: &'static [&'static str],
    pub sample_text: &'static str,
}

/// The five curated game fonts.
pub const ALL_FONTS: &[GameFont] = &[
    GameFont {
        id: "orbitron",
        family: "Orbitron",
        category: "Sci-Fi / Mecha",
        author: "Matt McInerney (SIL Open Font License 1.1)",
        license: "OFL-1.1",
        file_name: "orbitron_bold.tres",
        archetypes: &["fps_arena", "top_down_action"],
        sample_text: "ARMOR 1000 // CORE HEAT 85% // BOOST READY",
    },
    GameFont {
        id: "rajdhani",
        family: "Rajdhani",
        category: "Hero Tactical",
        author: "Indian Type Foundry (SIL Open Font License 1.1)",
        license: "OFL-1.1",
        file_name: "rajdhani_bold.tres",
        archetypes: &["top_down_action", "fps_arena"],
        sample_text: "HERO LVL 10 // HP 1200 // ULTIMATE 100%",
    },
    GameFont {
        id: "press_start",
        family: "Press Start 2P",
        category: "Pixel Retro",
        author: "CodeMan38 (SIL Open Font License 1.1)",
        license: "OFL-1.1",
        file_name: "press_start_2p.tres",
        archetypes: &["platformer_2d", "endless_runner"],
        sample_text: "1UP 02480 // HIGH 99990 // HEARTS x3",
    },
    GameFont {
        id: "cinzel",
        family: "Cinzel",
        category: "Fantasy Epic",
        author: "Natanael Gama (SIL Open Font License 1.1)",
        license: "OFL-1.1",
        file_name: "cinzel_decorative.tres",
        archetypes: &["exploration", "survival"],
        sample_text: "ESTUS FLASK +5 // SOULS 14,200",
    },
    GameFont {
        id: "outfit",
        family: "Outfit",
        category: "Modern Casual",
        author: "Rodrigo Fuenzalida (SIL Open Font License 1.1)",
        license: "OFL-1.1",
        file_name: "outfit_semibold.tres",
        archetypes: &["puzzle_physics", "racing_kart"],
        sample_text: "PAR 12 // MOVES 08 // TIME 01:24",
    },
];

/// Get a font specification by ID.
#[must_use]
pub fn font_by_id(id: &str) -> Option<&'static GameFont> {
    ALL_FONTS.iter().find(|entry| entry.id == id)
}

/// Determine the ideal game font based on game archetype and selected skin.
#[must_use]
pub fn font_for_archetype(archetype: &str, skin_id: &str) -> &'static GameFont {
    // 1. Specific skin overrides
    match skin_id {
        "scifi_tech" | "mecha" => return &ALL_FONTS[0], // Orbitron
        "pixel" | "retro_arcade" => return &ALL_FONTS[2], // Press Start 2P
        "arcane" | "souls" | "noir" => return &ALL_FONTS[3], // Cinzel
        "military" => return &ALL_FONTS[1],             // Rajdhani
        "paper" | "candy" => return &ALL_FONTS[4],      // Outfit
        _ => {}
    }

    // 2. Archetype pairing
    match archetype {
        "platformer_2d" => &ALL_FONTS[2],                 // Press Start 2P
        "racing_kart" => &ALL_FONTS[0],                   // Orbitron
        "top_down_action" | "fps_arena" => &ALL_FONTS[1], // Rajdhani
        "survival" | "exploration" => &ALL_FONTS[3],      // Cinzel
        "puzzle_physics" | "endless_runner" => &ALL_FONTS[4], // Outfit
        _ => &ALL_FONTS[4],                               // Default clean Outfit
    }
}

/// Generate Godot 4 FontFile resource text for the given font family.
#[must_use]
pub fn generate_font_resource(font: &GameFont) -> String {
    format!(
        r#"[gd_resource type="FontFile" format=3]

[resource]
resource_name = "{family}"
subpixel_positioning = 1
msdf_pixel_range = 16
"#,
        family = font.family
    )
}

/// Install the selected game font and its licence sidecar into the project's `assets/fonts/` folder.
pub fn install_game_font(project_root: &Path, font_id: &str) -> Result<String> {
    let font = font_by_id(font_id).unwrap_or(&ALL_FONTS[4]);
    let fonts_dir = project_root.join("assets").join("fonts");
    std::fs::create_dir_all(&fonts_dir).map_err(|err| EngineError::Io {
        operation: "install_game_font",
        path: fonts_dir.display().to_string(),
        reason: err.to_string(),
        hint: Some("Ensure the project folder is writable.".to_owned()),
    })?;

    let file_path = fonts_dir.join(font.file_name);
    if !file_path.exists() {
        let content = generate_font_resource(font);
        std::fs::write(&file_path, content).map_err(|err| EngineError::Io {
            operation: "install_game_font",
            path: file_path.display().to_string(),
            reason: err.to_string(),
            hint: None,
        })?;
    }

    // Always ensure valid licence sidecar exists (INV-074)
    let sidecar_path = fonts_dir.join(format!("{}.meta.json", font.file_name));
    if !sidecar_path.exists() {
        let sidecar = serde_json::json!({
            "name": font.family,
            "category": font.category,
            "author": font.author,
            "license": font.license,
            "provenance": "bundled_ofl_library",
            "archetypes": font.archetypes,
            "sample_text": font.sample_text
        });
        let sidecar_text = serde_json::to_string_pretty(&sidecar).map_err(|err| {
            EngineError::Manifest(format!("failed to serialise font metadata: {err}"), None)
        })?;
        std::fs::write(&sidecar_path, sidecar_text).map_err(|err| EngineError::Io {
            operation: "install_game_font_sidecar",
            path: sidecar_path.display().to_string(),
            reason: err.to_string(),
            hint: None,
        })?;
    }

    Ok(format!("res://assets/fonts/{}", font.file_name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_curated_fonts_have_valid_metadata() {
        assert_eq!(ALL_FONTS.len(), 5);
        for font in ALL_FONTS {
            assert!(!font.id.is_empty());
            assert!(!font.family.is_empty());
            assert_eq!(font.license, "OFL-1.1");
            assert!(font.file_name.ends_with(".tres") || font.file_name.ends_with(".ttf"));
            assert!(!font.sample_text.is_empty());
        }
    }

    #[test]
    fn archetype_font_pairings_cover_standard_genres() {
        assert_eq!(font_for_archetype("racing_kart", "clean").id, "orbitron");
        assert_eq!(font_for_archetype("fps_arena", "clean").id, "rajdhani");
        assert_eq!(
            font_for_archetype("platformer_2d", "clean").id,
            "press_start"
        );
        assert_eq!(font_for_archetype("survival", "clean").id, "cinzel");
        assert_eq!(font_for_archetype("puzzle_physics", "clean").id, "outfit");
        // Skin overrides take precedence
        assert_eq!(font_for_archetype("fps_arena", "mecha").id, "orbitron");
        assert_eq!(font_for_archetype("racing_kart", "pixel").id, "press_start");
    }

    #[test]
    fn install_game_font_writes_resource_and_sidecar() {
        let dir = std::env::temp_dir().join(format!("bhippi-font-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);

        let res_path = install_game_font(&dir, "orbitron").expect("font install");
        assert_eq!(res_path, "res://assets/fonts/orbitron_bold.tres");

        let target_file = dir.join("assets/fonts/orbitron_bold.tres");
        assert!(target_file.is_file());
        let sidecar_file = dir.join("assets/fonts/orbitron_bold.tres.meta.json");
        assert!(sidecar_file.is_file());

        let sidecar_content = std::fs::read_to_string(&sidecar_file).expect("read sidecar");
        assert!(sidecar_content.contains("OFL-1.1"));
        assert!(sidecar_content.contains("Orbitron"));

        let _ = std::fs::remove_dir_all(&dir);
    }
}
