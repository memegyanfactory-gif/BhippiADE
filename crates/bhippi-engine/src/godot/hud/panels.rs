//! Procedural 9-slice HUD panel frames, boxes, and button styles (INV-074, GAD-160).
//!
//! Synthesizes resolution-independent SVG 9-slice frame textures matching four distinct
//! aesthetic visual traditions:
//! 1. `scifi_wireframe` (Cyberpunk / Diagnostics): 45° chamfered cut-corner frames with neon border
//!    brackets and tech tick lines (Freepik style).
//! 2. `hero_hex` (Hero Combat / MOBA): Hexagonal avatar portrait borders and slanted skill slots (Overwatch style).
//! 3. `casual_glossy` (Quest Tree / Casual): Rounded glossy embossed reward boxes and pill buttons (Empire City style).
//! 4. `brawl_pill` (Mobile Arena): Chunky 3D drop-shadow action buttons and banner containers (Brawl Stars style).
//! 5. `retro_pixel` (Pixel RPG): 2px stepped bitmap pixel borders for retro games.

use crate::error::{EngineError, Result};
use std::collections::BTreeMap;
use std::path::Path;

/// Identifiers for the procedural panel style families.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HudPanelStyle {
    SciFiWireframe,
    HeroHex,
    CasualGlossy,
    BrawlPill,
    RetroPixel,
}

impl HudPanelStyle {
    #[must_use]
    pub fn parse(name: &str) -> Self {
        match name.to_ascii_lowercase().as_str() {
            "scifi_wireframe" | "scifi_tech" | "mecha" | "neon" => Self::SciFiWireframe,
            "hero_hex" | "hero" | "moba" => Self::HeroHex,
            "casual_glossy" | "candy" | "paper" => Self::CasualGlossy,
            "brawl_pill" | "arena" => Self::BrawlPill,
            "retro_pixel" | "pixel" | "retro_arcade" => Self::RetroPixel,
            _ => Self::SciFiWireframe,
        }
    }
}

/// Generates an SVG 9-slice panel frame texture for the given style and accent colors.
#[must_use]
pub fn generate_panel_svg(style: HudPanelStyle, accent_hex: &str, plate_hex: &str) -> String {
    match style {
        HudPanelStyle::SciFiWireframe => format!(
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64" width="64" height="64">
  <!-- Cut-corner 45-degree chamfered sci-fi panel frame -->
  <polygon points="12,2 52,2 62,12 62,52 52,62 12,62 2,52 2,12" fill="{plate_hex}" fill-opacity="0.85" stroke="{accent_hex}" stroke-width="2"/>
  <!-- Corner tech notches -->
  <line x1="2" y1="12" x2="12" y2="2" stroke="{accent_hex}" stroke-width="3"/>
  <line x1="52" y1="2" x2="62" y2="12" stroke="{accent_hex}" stroke-width="3"/>
  <line x1="62" y1="52" x2="52" y2="62" stroke="{accent_hex}" stroke-width="3"/>
  <line x1="12" y1="62" x2="2" y2="52" stroke="{accent_hex}" stroke-width="3"/>
  <!-- Subtle inner guide line -->
  <line x1="14" y1="6" x2="50" y2="6" stroke="{accent_hex}" stroke-width="1" stroke-opacity="0.5"/>
</svg>"##
        ),
        HudPanelStyle::HeroHex => format!(
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64" width="64" height="64">
  <!-- Hexagonal hero badge frame with accented border -->
  <polygon points="32,2 60,18 60,46 32,62 4,46 4,18" fill="{plate_hex}" fill-opacity="0.9" stroke="{accent_hex}" stroke-width="3"/>
  <polygon points="32,6 56,20 56,44 32,58 8,44 8,20" fill="none" stroke="#ffffff" stroke-width="1" stroke-opacity="0.35"/>
  <!-- Bottom level badge notch -->
  <circle cx="32" cy="56" r="6" fill="{accent_hex}"/>
</svg>"##
        ),
        HudPanelStyle::CasualGlossy => format!(
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64" width="64" height="64">
  <defs>
    <linearGradient id="gloss" x1="0" y1="0" x2="0" y2="1">
      <stop offset="0%" stop-color="#ffffff" stop-opacity="0.4"/>
      <stop offset="50%" stop-color="#ffffff" stop-opacity="0.05"/>
      <stop offset="100%" stop-color="#000000" stop-opacity="0.2"/>
    </linearGradient>
  </defs>
  <!-- 3D beveled rounded plate with drop shadow -->
  <rect x="2" y="5" width="60" height="56" rx="10" fill="#000000" fill-opacity="0.3"/>
  <rect x="2" y="2" width="60" height="56" rx="10" fill="{plate_hex}" stroke="{accent_hex}" stroke-width="2.5"/>
  <!-- Gloss overlay -->
  <rect x="4" y="4" width="56" height="26" rx="8" fill="url(#gloss)"/>
</svg>"##
        ),
        HudPanelStyle::BrawlPill => format!(
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64" width="64" height="64">
  <!-- Chunky solid cartoon action box with dark bottom stroke -->
  <rect x="2" y="6" width="60" height="54" rx="8" fill="#111122"/>
  <rect x="2" y="2" width="60" height="54" rx="8" fill="{plate_hex}" stroke="{accent_hex}" stroke-width="3"/>
  <line x1="6" y1="5" x2="58" y2="5" stroke="#ffffff" stroke-width="2" stroke-opacity="0.5" stroke-linecap="round"/>
</svg>"##
        ),
        HudPanelStyle::RetroPixel => format!(
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64" width="64" height="64" shape-rendering="crispEdges">
  <!-- Stepped 2px pixel border -->
  <rect x="0" y="0" width="64" height="64" fill="{plate_hex}"/>
  <rect x="0" y="0" width="64" height="4" fill="{accent_hex}"/>
  <rect x="0" y="60" width="64" height="4" fill="{accent_hex}"/>
  <rect x="0" y="0" width="4" height="64" fill="{accent_hex}"/>
  <rect x="60" y="0" width="4" height="64" fill="{accent_hex}"/>
  <!-- Inner shadow -->
  <rect x="4" y="4" width="56" height="2" fill="#ffffff" opacity="0.3"/>
  <rect x="4" y="58" width="56" height="2" fill="#000000" opacity="0.4"/>
</svg>"##
        ),
    }
}

/// Generates a button SVG with normal and pressed states.
#[must_use]
pub fn generate_button_svg(style: HudPanelStyle, accent_hex: &str, is_pressed: bool) -> String {
    let offset_y = if is_pressed { 4 } else { 0 };
    match style {
        HudPanelStyle::CasualGlossy | HudPanelStyle::BrawlPill => format!(
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 96 40" width="96" height="40">
  <!-- 3D Action button plate -->
  <rect x="2" y="8" width="92" height="30" rx="12" fill="#0a0a14" fill-opacity="0.6"/>
  <rect x="2" y="{y}" width="92" height="30" rx="12" fill="{accent_hex}" stroke="#ffffff" stroke-width="1.5" stroke-opacity="0.6"/>
  <line x1="12" y1="{h_y}" x2="84" y2="{h_y}" stroke="#ffffff" stroke-width="2" stroke-opacity="0.5" stroke-linecap="round"/>
</svg>"##,
            y = 2 + offset_y,
            h_y = 5 + offset_y
        ),
        _ => format!(
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 96 40" width="96" height="40">
  <!-- Angular high-tech action button -->
  <polygon points="10,2 86,2 94,10 94,30 86,38 10,38 2,30 2,10" fill="{fill}" stroke="{accent_hex}" stroke-width="2"/>
  <line x1="8" y1="5" x2="88" y2="5" stroke="{accent_hex}" stroke-width="1" stroke-opacity="0.6"/>
</svg>"##,
            fill = if is_pressed { accent_hex } else { "#141a24" }
        ),
    }
}

/// Install panel and button SVG frames into the project's `assets/ui/panels/` directory.
pub fn install_hud_panels(
    project_root: &Path,
    style_name: &str,
    accent_hex: &str,
    plate_hex: &str,
) -> Result<BTreeMap<String, String>> {
    let style = HudPanelStyle::parse(style_name);
    let target_dir = project_root.join("assets").join("ui").join("panels");
    std::fs::create_dir_all(&target_dir).map_err(|err| EngineError::Io {
        operation: "install_hud_panels",
        path: target_dir.display().to_string(),
        reason: err.to_string(),
        hint: Some("Check folder permissions.".to_owned()),
    })?;

    let mut map = BTreeMap::new();

    // 1. Panel frame
    let panel_filename = format!("panel_{style_name}.svg");
    let panel_path = target_dir.join(&panel_filename);
    if !panel_path.exists() {
        let svg = generate_panel_svg(style, accent_hex, plate_hex);
        std::fs::write(&panel_path, svg).map_err(|err| EngineError::Io {
            operation: "write_panel_svg",
            path: panel_path.display().to_string(),
            reason: err.to_string(),
            hint: None,
        })?;
    }
    map.insert(
        "panel_frame".to_owned(),
        format!("res://assets/ui/panels/{panel_filename}"),
    );

    // 2. Button normal
    let btn_normal = format!("button_{style_name}_normal.svg");
    let btn_normal_path = target_dir.join(&btn_normal);
    if !btn_normal_path.exists() {
        let svg = generate_button_svg(style, accent_hex, false);
        std::fs::write(&btn_normal_path, svg).map_err(|err| EngineError::Io {
            operation: "write_button_normal",
            path: btn_normal_path.display().to_string(),
            reason: err.to_string(),
            hint: None,
        })?;
    }
    map.insert(
        "button_normal".to_owned(),
        format!("res://assets/ui/panels/{btn_normal}"),
    );

    // 3. Button pressed
    let btn_pressed = format!("button_{style_name}_pressed.svg");
    let btn_pressed_path = target_dir.join(&btn_pressed);
    if !btn_pressed_path.exists() {
        let svg = generate_button_svg(style, accent_hex, true);
        std::fs::write(&btn_pressed_path, svg).map_err(|err| EngineError::Io {
            operation: "write_button_pressed",
            path: btn_pressed_path.display().to_string(),
            reason: err.to_string(),
            hint: None,
        })?;
    }
    map.insert(
        "button_pressed".to_owned(),
        format!("res://assets/ui/panels/{btn_pressed}"),
    );

    // Sidecar metadata
    let sidecar_path = target_dir.join(format!("panels_{style_name}.meta.json"));
    if !sidecar_path.exists() {
        let sidecar = serde_json::json!({
            "name": format!("HUD Panels: {style_name}"),
            "license": "CC0-1.0",
            "author": "Bhippi Engine (CC0 1.0 Universal)",
            "provenance": "procedural_hud_panels",
            "style": style_name
        });
        std::fs::write(
            &sidecar_path,
            serde_json::to_string_pretty(&sidecar).map_err(|e| {
                EngineError::Manifest(format!("failed to serialise panel metadata: {e}"), None)
            })?,
        )
        .map_err(|err| EngineError::Io {
            operation: "write_panel_sidecar",
            path: sidecar_path.display().to_string(),
            reason: err.to_string(),
            hint: None,
        })?;
    }

    Ok(map)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_panel_styles_generate_valid_svg() {
        let styles = [
            HudPanelStyle::SciFiWireframe,
            HudPanelStyle::HeroHex,
            HudPanelStyle::CasualGlossy,
            HudPanelStyle::BrawlPill,
            HudPanelStyle::RetroPixel,
        ];
        for style in styles {
            let svg = generate_panel_svg(style, "#00e5ff", "#141a24");
            assert!(svg.starts_with("<svg"), "missing svg open tag");
            assert!(svg.ends_with("</svg>"), "missing svg close tag");
            assert!(svg.contains("#00e5ff"));
        }
    }

    #[test]
    fn button_styles_generate_normal_and_pressed_states() {
        let normal = generate_button_svg(HudPanelStyle::CasualGlossy, "#ff9100", false);
        let pressed = generate_button_svg(HudPanelStyle::CasualGlossy, "#ff9100", true);
        assert_ne!(normal, pressed);
        assert!(normal.contains("</svg>"));
        assert!(pressed.contains("</svg>"));
    }

    #[test]
    fn install_hud_panels_writes_all_three_textures_and_sidecar() {
        let dir = std::env::temp_dir().join(format!("bhippi-panel-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);

        let map = install_hud_panels(&dir, "scifi_tech", "#00e5ff", "#141a24").expect("install");
        assert_eq!(map.len(), 3);
        assert!(map.contains_key("panel_frame"));
        assert!(map.contains_key("button_normal"));
        assert!(map.contains_key("button_pressed"));

        let panel_file = dir.join("assets/ui/panels/panel_scifi_tech.svg");
        assert!(panel_file.is_file());
        let meta_file = dir.join("assets/ui/panels/panels_scifi_tech.meta.json");
        assert!(meta_file.is_file());

        let _ = std::fs::remove_dir_all(&dir);
    }
}
