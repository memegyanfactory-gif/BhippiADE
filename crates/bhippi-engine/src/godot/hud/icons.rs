//! Built-in local icon library for the HUD system.
//!
//! Provides embedded, zero-dependency SVG icons for all standard HUD roles across two
//! distinct styles:
//! * `clean`: Smooth modern vector shapes with crisp geometry.
//! * `pixel`: Chunky 16x16 pixel-grid silhouettes for retro RPGs, platformers and arcade games.
//!
//! When a game has no external asset pack installed, [`install_local_icons`] writes these
//! directly to `assets/ui/icons/` so the HUD always has crisp icons and never ships broken
//! or empty `TextureRect` nodes.

use crate::error::{EngineError, Result};
use std::collections::BTreeMap;
use std::path::Path;

/// All roles supported by the local icon library.
pub const ALL_ROLES: &[&str] = &[
    "heart", "shield", "mana", "stamina", "coin", "gem", "star", "clock", "ammo", "bolt", "food",
    "eye", "key", "sword", "potion", "fuel", "speed", "skull", "bomb", "compass", "trophy",
    "target", "badge", "diamond",
];

/// Get the raw SVG text for a given role and style.
/// Style can be "pixel" or "clean" (defaults to "clean").
#[must_use]
pub fn get_icon_svg(role: &str, style: &str) -> Option<&'static str> {
    let is_pixel = style.eq_ignore_ascii_case("pixel") || style.eq_ignore_ascii_case("retro");
    if is_pixel {
        pixel_svg(role)
    } else {
        clean_svg(role)
    }
}

/// Get the raw SVG text for a given role and style, synthesizing a clean SVG if not in the default catalogue.
#[must_use]
pub fn get_or_synthesize_icon_svg(role: &str, style: &str) -> String {
    if let Some(svg) = get_icon_svg(role, style) {
        svg.to_owned()
    } else {
        super::generator::synthesize_icon_svg(role, style)
    }
}

/// Write missing HUD icons into the project's icon directory using the local built-in library.
pub fn install_local_icons(
    project_root: &Path,
    icon_dir: &str,
    roles: &[&str],
    style: &str,
) -> Result<BTreeMap<String, String>> {
    let target_dir = project_root.join(icon_dir.replace('\\', "/"));
    std::fs::create_dir_all(&target_dir).map_err(|err| EngineError::Io {
        operation: "install_local_icons",
        path: target_dir.display().to_string(),
        reason: err.to_string(),
        hint: Some("Check the parent folder is writable.".to_owned()),
    })?;

    let mut installed = BTreeMap::new();
    for &role in roles {
        let file_name = format!("{role}.svg");
        let file_path = target_dir.join(&file_name);
        // Do not overwrite an icon the developer or an asset pack already provided
        if !file_path.exists() {
            let svg_content = get_or_synthesize_icon_svg(role, style);
            std::fs::write(&file_path, svg_content).map_err(|err| EngineError::Io {
                operation: "install_local_icons",
                path: file_path.display().to_string(),
                reason: err.to_string(),
                hint: None,
            })?;
        }
        if file_path.is_file() {
            installed.insert(
                role.to_owned(),
                format!("res://{}/{file_name}", icon_dir.trim_end_matches('/')),
            );
        }
    }

    Ok(installed)
}

// ---------------------------------------------------------------------------- Clean Vector SVGs

fn clean_svg(role: &str) -> Option<&'static str> {
    match role {
        "heart" => Some(
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 32 32" width="32" height="32">
  <path fill="#ff3366" d="M16 28.5l-2.3-2.1C5.6 19.1 1 14.9 1 9.7 1 5.4 4.4 2 8.7 2c2.4 0 4.8 1.1 6.3 3 1.5-1.9 3.9-3 6.3-3C25.6 2 29 5.4 29 9.7c0 5.2-4.6 9.4-12.7 16.7L16 28.5z"/>
  <path fill="#ffffff" opacity="0.3" d="M9 5c-2.8 0-5 2.2-5 5 0 1.5.6 3 1.8 4.2L16 24l2-2-9-9c-1-1-1.5-2.2-1.5-3.5 0-1.9 1.6-3.5 3.5-3.5H9z"/>
</svg>"##,
        ),
        "shield" => Some(
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 32 32" width="32" height="32">
  <path fill="#3399ff" d="M16 2L4 7v9c0 8.3 5.1 13.6 12 15 6.9-1.4 12-6.7 12-15V7L16 2z"/>
  <path fill="#66ccff" d="M16 5l9 3.8v7.2c0 6.6-4.1 10.9-9 12V5z"/>
  <path fill="#ffffff" opacity="0.35" d="M7 9v7c0 6.6 4.1 10.9 9 12V5L7 9z"/>
</svg>"##,
        ),
        "mana" => Some(
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 32 32" width="32" height="32">
  <path fill="#9933ff" d="M16 2C16 2 6 14 6 21a10 10 0 0 0 20 0C26 14 16 2 16 2z"/>
  <path fill="#cc66ff" d="M16 6c0 0 7 8 7 15a7 7 0 0 1-7 7V6z"/>
  <circle cx="12" cy="18" r="3" fill="#ffffff" opacity="0.6"/>
</svg>"##,
        ),
        "stamina" => Some(
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 32 32" width="32" height="32">
  <path fill="#33cc66" d="M18 2L5 18h9l-2 12 15-18h-10l3-10z"/>
  <path fill="#88ffaa" opacity="0.5" d="M18 2l-1 6h6l-9 10 1-5h-5l10-11z"/>
</svg>"##,
        ),
        "coin" => Some(
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 32 32" width="32" height="32">
  <circle cx="16" cy="16" r="14" fill="#ffcc00"/>
  <circle cx="16" cy="16" r="11" fill="#e6b800"/>
  <circle cx="16" cy="16" r="8" fill="#ffdb4d"/>
  <path fill="#b38f00" d="M15 10h2v12h-2zm-3 2h6v2h-6zm0 8h6v2h-6z"/>
</svg>"##,
        ),
        "gem" | "diamond" => Some(
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 32 32" width="32" height="32">
  <polygon fill="#00e5ff" points="8,4 24,4 30,12 16,30 2,12"/>
  <polygon fill="#80f3ff" points="8,4 24,4 21,12 11,12"/>
  <polygon fill="#ffffff" opacity="0.6" points="8,4 11,12 2,12"/>
  <polygon fill="#00b8d4" points="16,30 21,12 30,12"/>
  <polygon fill="#0091ea" points="16,30 11,12 21,12"/>
</svg>"##,
        ),
        "star" => Some(
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 32 32" width="32" height="32">
  <polygon fill="#ffb300" points="16,2 20.6,11.3 30.9,12.8 23.4,20.1 25.2,30.3 16,25.5 6.8,30.3 8.6,20.1 1.1,12.8 11.4,11.3"/>
  <polygon fill="#ffe082" opacity="0.7" points="16,4 19.5,12 27,13 21,18.5 22.5,26 16,22"/>
</svg>"##,
        ),
        "clock" => Some(
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 32 32" width="32" height="32">
  <circle cx="16" cy="16" r="14" fill="#37474f"/>
  <circle cx="16" cy="16" r="11.5" fill="#eceff1"/>
  <path fill="#263238" d="M15 8h2v8.5l5.5 3.3-1 1.7L15 17.5z"/>
  <circle cx="16" cy="16" r="2" fill="#d32f2f"/>
</svg>"##,
        ),
        "ammo" => Some(
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 32 32" width="32" height="32">
  <path fill="#ff9800" d="M10 12c0-4 3-8 6-8s6 4 6 8v14h-12V12z"/>
  <path fill="#ffb74d" d="M12 12c0-3 2-6 4-6v18h-4V12z"/>
  <rect x="8" y="26" width="16" height="4" rx="1" fill="#795548"/>
</svg>"##,
        ),
        "bolt" => Some(
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 32 32" width="32" height="32">
  <path fill="#ffd600" d="M19 2L5 17h10l-3 13 15-16H16l4-12z"/>
  <path fill="#fff9c4" opacity="0.6" d="M19 2l-1 5h4l-9 10 1-5h-4l9-10z"/>
</svg>"##,
        ),
        "food" => Some(
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 32 32" width="32" height="32">
  <path fill="#e53935" d="M16 6c-6 0-11 4.5-11 11a11 11 0 0 0 22 0c0-6.5-5-11-11-11z"/>
  <path fill="#4caf50" d="M16 2c2 0 4 1 5 3-2 1-4 1-6 0v-3z"/>
  <path fill="#ffffff" opacity="0.3" d="M10 11a7 7 0 0 1 7-4v2a5 5 0 0 0-5 5h-2z"/>
</svg>"##,
        ),
        "eye" => Some(
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 32 32" width="32" height="32">
  <path fill="#cfd8dc" d="M16 6C8 6 2 16 2 16s6 10 14 10 14-10 14-10-6-10-14-10z"/>
  <circle cx="16" cy="16" r="6" fill="#00bcd4"/>
  <circle cx="16" cy="16" r="3" fill="#212121"/>
  <circle cx="14.5" cy="14.5" r="1.5" fill="#ffffff"/>
</svg>"##,
        ),
        "key" => Some(
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 32 32" width="32" height="32">
  <circle cx="11" cy="13" r="8" fill="#ffd54f"/>
  <circle cx="11" cy="13" r="4" fill="#212121"/>
  <rect x="15" y="11" width="14" height="4" rx="1" fill="#ffca28"/>
  <rect x="23" y="15" width="3" height="4" rx="0.5" fill="#ffca28"/>
  <rect x="27" y="15" width="2" height="5" rx="0.5" fill="#ffca28"/>
</svg>"##,
        ),
        "sword" => Some(
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 32 32" width="32" height="32">
  <path fill="#e0e0e0" d="M28 4l-4 0-14 14 2 2 14-14 2-2z"/>
  <path fill="#ffffff" opacity="0.5" d="M28 4l-2 0-14 14 1 1 14-14 1-1z"/>
  <path fill="#78909c" d="M11 17l-3-3-2 2 3 3 2-2z"/>
  <path fill="#8d6e63" d="M8 20l-4 4 2 2 4-4-2-2z"/>
  <circle cx="4" cy="28" r="2" fill="#ffd54f"/>
</svg>"##,
        ),
        "potion" => Some(
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 32 32" width="32" height="32">
  <rect x="13" y="3" width="6" height="3" rx="1" fill="#8d6e63"/>
  <rect x="14" y="6" width="4" height="4" fill="#b0bec5"/>
  <path fill="#e91e63" d="M16 9l-7 9a6 6 0 0 0 4 10h6a6 6 0 0 0 4-10l-7-9z"/>
  <circle cx="14" cy="20" r="2" fill="#ffffff" opacity="0.6"/>
</svg>"##,
        ),
        "fuel" => Some(
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 32 32" width="32" height="32">
  <rect x="5" y="6" width="15" height="22" rx="2" fill="#e53935"/>
  <rect x="8" y="10" width="9" height="7" rx="1" fill="#eceff1"/>
  <path fill="none" stroke="#e53935" stroke-width="2.5" d="M20 12h2a3 3 0 0 1 3 3v8a2 2 0 0 0 2 2"/>
</svg>"##,
        ),
        "speed" => Some(
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 32 32" width="32" height="32">
  <path fill="none" stroke="#37474f" stroke-width="3" stroke-linecap="round" d="M6 24 A13 13 0 1 1 26 24"/>
  <path fill="none" stroke="#00e676" stroke-width="3" stroke-linecap="round" d="M6 24 A13 13 0 0 1 19 3"/>
  <line x1="16" y1="18" x2="23" y2="10" stroke="#ff1744" stroke-width="2" stroke-linecap="round"/>
  <circle cx="16" cy="18" r="3" fill="#cfd8dc"/>
</svg>"##,
        ),
        "skull" => Some(
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 32 32" width="32" height="32">
  <path fill="#eceff1" d="M16 3c-7 0-11 5-11 12 0 4 2 7 5 9v4h12v-4c3-2 5-5 5-9 0-7-4-12-11-12z"/>
  <circle cx="12" cy="14" r="3" fill="#263238"/>
  <circle cx="20" cy="14" r="3" fill="#263238"/>
  <polygon fill="#263238" points="16,18 14.5,21 17.5,21"/>
  <line x1="13" y1="25" x2="13" y2="28" stroke="#263238" stroke-width="1.5"/>
  <line x1="16" y1="25" x2="16" y2="28" stroke="#263238" stroke-width="1.5"/>
  <line x1="19" y1="25" x2="19" y2="28" stroke="#263238" stroke-width="1.5"/>
</svg>"##,
        ),
        "bomb" => Some(
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 32 32" width="32" height="32">
  <circle cx="15" cy="18" r="11" fill="#263238"/>
  <rect x="18" y="6" width="4" height="4" fill="#78909c" transform="rotate(30 20 8)"/>
  <path fill="none" stroke="#ff9800" stroke-width="2" d="M22 6c2-3 4-2 6-4"/>
  <circle cx="28" cy="2" r="2" fill="#ffeb3b"/>
  <circle cx="11" cy="14" r="2.5" fill="#ffffff" opacity="0.4"/>
</svg>"##,
        ),
        "compass" => Some(
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 32 32" width="32" height="32">
  <circle cx="16" cy="16" r="14" fill="none" stroke="#90a4ae" stroke-width="2"/>
  <polygon fill="#e53935" points="16,5 20,16 16,14"/>
  <polygon fill="#cfd8dc" points="16,27 20,16 16,18"/>
  <polygon fill="#b71c1c" points="16,5 12,16 16,14"/>
  <polygon fill="#78909c" points="16,27 12,16 16,18"/>
</svg>"##,
        ),
        "trophy" => Some(
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 32 32" width="32" height="32">
  <path fill="#ffd54f" d="M7 4h18v9a9 9 0 0 1-18 0V4z"/>
  <path fill="#ffca28" d="M7 6H3a3 3 0 0 0 3 3h1V6zm18 0h4a3 3 0 0 1-3 3h-1V6z"/>
  <rect x="14" y="20" width="4" height="5" fill="#ffb300"/>
  <rect x="10" y="25" width="12" height="4" rx="1" fill="#795548"/>
</svg>"##,
        ),
        "target" => Some(
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 32 32" width="32" height="32">
  <circle cx="16" cy="16" r="11" fill="none" stroke="#ff3d00" stroke-width="2"/>
  <circle cx="16" cy="16" r="4" fill="#ff3d00"/>
  <line x1="16" y1="2" x2="16" y2="9" stroke="#ff3d00" stroke-width="2"/>
  <line x1="16" y1="23" x2="16" y2="30" stroke="#ff3d00" stroke-width="2"/>
  <line x1="2" y1="16" x2="9" y2="16" stroke="#ff3d00" stroke-width="2"/>
  <line x1="23" y1="16" x2="30" y2="16" stroke="#ff3d00" stroke-width="2"/>
</svg>"##,
        ),
        "badge" => Some(
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 32 32" width="32" height="32">
  <polygon fill="#7c4dff" points="16,2 28,8 28,24 16,30 4,24 4,8"/>
  <polygon fill="#b388ff" points="16,5 25,10 25,22 16,27 7,22 7,10"/>
  <circle cx="16" cy="16" r="5" fill="#ffd700"/>
</svg>"##,
        ),
        _ => None,
    }
}

// ---------------------------------------------------------------------------- Pixel Art SVGs

fn pixel_svg(role: &str) -> Option<&'static str> {
    match role {
        "heart" => Some(
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16" width="32" height="32" shape-rendering="crispEdges">
  <path fill="#200000" d="M2 3h4v1H2zm6 0h4v1H8zM1 4h1v3H1zm5 4h1v1H6zm4 0h1v1h-1zm3-4h1v3h-1zM2 7h1v2H2zm10 0h1v2h-1zm-9 2h1v2H3zm8 0h1v2h-1zm-7 2h1v2H4zm6 0h1v2h-1zm-5 2h1v1H5zm4 0h1v1H9zm-3 1h2v1H6z"/>
  <path fill="#e60033" d="M2 4h4v3H2zm6 0h4v3H8zM3 7h10v2H3zm1 2h8v2H4zm1 2h6v1H5zm1 1h4v1H6zm1 1h2v1H7z"/>
  <rect x="3" y="4" width="2" height="2" fill="#ff6688"/>
</svg>"##,
        ),
        "shield" => Some(
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16" width="32" height="32" shape-rendering="crispEdges">
  <path fill="#1a3366" d="M1 2h14v2H1zm0 2h2v7H1zm12 0h2v7h-2zM3 11h2v2H3zm8 0h2v2h-2zm-6 2h2v2H5zm4 0h2v2H9zm-2 2h2v1H7z"/>
  <path fill="#3399ff" d="M3 4h10v7H3zm2 7h6v2H5zm2 2h2v1H7z"/>
  <path fill="#80c4ff" d="M4 4h4v6H4zm1 7h3v1H5z"/>
</svg>"##,
        ),
        "mana" => Some(
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16" width="32" height="32" shape-rendering="crispEdges">
  <path fill="#330066" d="M7 1h2v2H7zm-2 2h2v3H5zm4 0h2v3H9zm-3 3h2v5H6zm4 0h2v5h-2zm-4 5h2v3H6zm4 0h2v3h-2zm-2 3h2v1H8z"/>
  <path fill="#9933ff" d="M7 3h2v3H7zm-1 3h4v5H6zm1 5h2v3H7z"/>
  <rect x="7" y="5" width="1" height="2" fill="#e6ccff"/>
</svg>"##,
        ),
        "coin" => Some(
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16" width="32" height="32" shape-rendering="crispEdges">
  <path fill="#664400" d="M5 2h6v1H5zm-3 3h3v1H2zm9 0h3v1h-3zM1 6h1v5H1zm13 0h1v5h-1zM2 11h3v1H2zm9 0h3v1h-3zm-6 3h6v1H5z"/>
  <path fill="#ffcc00" d="M5 3h6v1H5zM2 6h12v5H2zm3 5h6v1H5z"/>
  <path fill="#ffeb80" d="M4 4h4v1H4zm-1 2h2v4H3z"/>
  <rect x="7" y="6" width="2" height="4" fill="#cc9900"/>
</svg>"##,
        ),
        "gem" | "diamond" => Some(
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16" width="32" height="32" shape-rendering="crispEdges">
  <path fill="#003344" d="M4 2h8v1H4zm-3 3h3v1H1zm11 0h3v1h-3zM0 6h1v2H0zm14 0h1v2h-1zM1 8h2v2H1zm11 0h2v2h-2zm-9 2h2v2H3zm7 0h2v2h-2zm-5 2h2v2H5zm3 0h2v2H8zm-2 2h2v1H6z"/>
  <path fill="#00e5ff" d="M4 3h8v3H4zM1 6h14v2H1zm2 2h10v2H3zm2 2h6v2H5zm1 2h4v1H6z"/>
  <path fill="#ccf9ff" d="M5 3h4v3H5zm-3 3h3v2H2z"/>
</svg>"##,
        ),
        "star" => Some(
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16" width="32" height="32" shape-rendering="crispEdges">
  <path fill="#664400" d="M7 1h2v3H7zm-3 4h8v2H4zM1 6h14v2H1zm2 2h12v2H3zm1 2h8v2H4zm-2 2h4v2H2zm8 0h4v2h-4z"/>
  <path fill="#ffcc00" d="M7 2h2v3H7zm-2 3h6v2H5zM2 7h12v1H2zm2 1h8v2H4zm1 2h6v1H5zm-2 2h3v1H3zm7 0h3v1h-3z"/>
  <rect x="7" y="4" width="2" height="2" fill="#fff5cc"/>
</svg>"##,
        ),
        "clock" => Some(
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16" width="32" height="32" shape-rendering="crispEdges">
  <path fill="#222" d="M5 1h6v1H5zm-3 2h3v1H2zm9 0h3v1h-3zM1 5h1v6H1zm13 0h1v6h-1zM2 12h3v1H2zm9 0h3v1h-3zm-6 2h6v1H5z"/>
  <path fill="#fff" d="M5 2h6v2H5zM2 4h12v8H2zm3 8h6v2H5z"/>
  <path fill="#cc0000" d="M7 4h2v4H7zm2 4h3v2H9z"/>
</svg>"##,
        ),
        "ammo" => Some(
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16" width="32" height="32" shape-rendering="crispEdges">
  <rect x="6" y="1" width="4" height="2" fill="#ff9900"/>
  <rect x="5" y="3" width="6" height="8" fill="#ffcc00"/>
  <rect x="4" y="11" width="8" height="3" fill="#996633"/>
  <rect x="6" y="3" width="1" height="8" fill="#fff2cc"/>
</svg>"##,
        ),
        "sword" => Some(
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16" width="32" height="32" shape-rendering="crispEdges">
  <rect x="13" y="1" width="2" height="2" fill="#fff"/>
  <rect x="11" y="3" width="3" height="3" fill="#e6e6e6"/>
  <rect x="9" y="5" width="3" height="3" fill="#ccc"/>
  <rect x="7" y="7" width="3" height="3" fill="#b3b3b3"/>
  <rect x="5" y="9" width="3" height="3" fill="#ffcc00"/>
  <rect x="3" y="7" width="3" height="2" fill="#ff9900"/>
  <rect x="7" y="11" width="2" height="3" fill="#ff9900"/>
  <rect x="3" y="11" width="3" height="3" fill="#663300"/>
  <rect x="1" y="13" width="3" height="3" fill="#ffcc00"/>
</svg>"##,
        ),
        "key" => Some(
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16" width="32" height="32" shape-rendering="crispEdges">
  <rect x="2" y="4" width="6" height="6" fill="#ffcc00"/>
  <rect x="4" y="6" width="2" height="2" fill="#111"/>
  <rect x="7" y="6" width="7" height="2" fill="#ffaa00"/>
  <rect x="11" y="8" width="2" height="3" fill="#ffaa00"/>
  <rect x="13" y="8" width="1" height="2" fill="#ffaa00"/>
</svg>"##,
        ),
        "fuel" => Some(
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16" width="32" height="32" shape-rendering="crispEdges">
  <rect x="2" y="2" width="8" height="12" fill="#e62e00"/>
  <rect x="4" y="4" width="4" height="4" fill="#ffffff"/>
  <rect x="9" y="4" width="3" height="2" fill="#111111"/>
  <rect x="11" y="6" width="2" height="6" fill="#111111"/>
  <rect x="12" y="11" width="2" height="2" fill="#111111"/>
</svg>"##,
        ),
        "skull" => Some(
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16" width="32" height="32" shape-rendering="crispEdges">
  <rect x="4" y="1" width="8" height="2" fill="#fff"/>
  <rect x="2" y="3" width="12" height="6" fill="#fff"/>
  <rect x="4" y="9" width="8" height="4" fill="#fff"/>
  <rect x="4" y="5" width="2" height="3" fill="#111"/>
  <rect x="10" y="5" width="2" height="3" fill="#111"/>
  <rect x="7" y="8" width="2" height="2" fill="#111"/>
  <rect x="6" y="12" width="1" height="2" fill="#111"/>
  <rect x="9" y="12" width="1" height="2" fill="#111"/>
</svg>"##,
        ),
        "potion" => Some(
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16" width="32" height="32" shape-rendering="crispEdges">
  <rect x="6" y="1" width="4" height="2" fill="#8d6e63"/>
  <rect x="7" y="3" width="2" height="2" fill="#ccc"/>
  <rect x="5" y="5" width="6" height="2" fill="#ccc"/>
  <rect x="3" y="7" width="10" height="7" fill="#e91e63"/>
  <rect x="5" y="13" width="6" height="1" fill="#e91e63"/>
  <rect x="5" y="8" width="2" height="2" fill="#fff"/>
</svg>"##,
        ),
        "eye" => Some(
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16" width="32" height="32" shape-rendering="crispEdges">
  <rect x="4" y="3" width="8" height="2" fill="#ccc"/>
  <rect x="2" y="5" width="12" height="6" fill="#fff"/>
  <rect x="4" y="11" width="8" height="2" fill="#ccc"/>
  <rect x="6" y="6" width="4" height="4" fill="#00bcd4"/>
  <rect x="7" y="7" width="2" height="2" fill="#111"/>
</svg>"##,
        ),
        "bolt" | "stamina" => Some(
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16" width="32" height="32" shape-rendering="crispEdges">
  <polygon fill="#ffeb3b" points="9,1 3,8 8,8 6,15 13,7 8,7"/>
</svg>"##,
        ),
        "speed" => Some(
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16" width="32" height="32" shape-rendering="crispEdges">
  <path fill="#37474f" d="M4 2h8v2H4zM2 4h2v8H2zm10 0h2v8h-2zM4 12h8v2H4z"/>
  <rect x="5" y="5" width="2" height="2" fill="#00e676"/>
  <rect x="7" y="4" width="2" height="2" fill="#00e676"/>
  <rect x="9" y="5" width="2" height="2" fill="#ff1744"/>
  <line x1="8" y1="8" x2="11" y2="5" stroke="#ff1744" stroke-width="2"/>
</svg>"##,
        ),
        "bomb" => Some(
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16" width="32" height="32" shape-rendering="crispEdges">
  <circle cx="8" cy="9" r="6" fill="#212121"/>
  <rect x="7" y="2" width="2" height="2" fill="#ff9800"/>
  <rect x="9" y="1" width="2" height="2" fill="#ffeb3b"/>
  <circle cx="6" cy="7" r="1.5" fill="#ffffff" opacity="0.6"/>
</svg>"##,
        ),
        "compass" => Some(
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16" width="32" height="32" shape-rendering="crispEdges">
  <circle cx="8" cy="8" r="7" fill="none" stroke="#607d8b" stroke-width="2"/>
  <polygon fill="#e53935" points="8,2 10,8 8,7"/>
  <polygon fill="#cfd8dc" points="8,14 10,8 8,9"/>
  <polygon fill="#b71c1c" points="8,2 6,8 8,7"/>
  <polygon fill="#78909c" points="8,14 6,8 8,9"/>
</svg>"##,
        ),
        "trophy" => Some(
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16" width="32" height="32" shape-rendering="crispEdges">
  <rect x="4" y="2" width="8" height="6" fill="#ffd54f"/>
  <rect x="2" y="3" width="2" height="3" fill="#ffca28"/>
  <rect x="12" y="3" width="2" height="3" fill="#ffca28"/>
  <rect x="7" y="8" width="2" height="4" fill="#ffb300"/>
  <rect x="5" y="12" width="6" height="2" fill="#8d6e63"/>
</svg>"##,
        ),
        "target" => Some(
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16" width="32" height="32" shape-rendering="crispEdges">
  <rect x="7" y="0" width="2" height="4" fill="#ff3d00"/>
  <rect x="7" y="12" width="2" height="4" fill="#ff3d00"/>
  <rect x="0" y="7" width="4" height="2" fill="#ff3d00"/>
  <rect x="12" y="7" width="4" height="2" fill="#ff3d00"/>
  <rect x="7" y="7" width="2" height="2" fill="#ff3d00"/>
</svg>"##,
        ),
        "badge" => Some(
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16" width="32" height="32" shape-rendering="crispEdges">
  <polygon fill="#7c4dff" points="8,1 14,4 14,12 8,15 2,12 2,4"/>
  <rect x="6" y="6" width="4" height="4" fill="#ffd700"/>
</svg>"##,
        ),
        "food" => Some(
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16" width="32" height="32" shape-rendering="crispEdges">
  <circle cx="8" cy="9" r="6" fill="#e53935"/>
  <rect x="7" y="1" width="2" height="3" fill="#4caf50"/>
  <rect x="5" y="6" width="2" height="2" fill="#ffffff" opacity="0.5"/>
</svg>"##,
        ),
        _ => clean_svg(role),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_role_has_valid_clean_svg() {
        for &role in ALL_ROLES {
            let svg = get_icon_svg(role, "clean")
                .unwrap_or_else(|| panic!("missing clean SVG for {role}"));
            assert!(svg.starts_with("<svg"), "{role} is not an svg: {svg}");
            assert!(svg.ends_with("</svg>"), "{role} does not close svg tag");
        }
    }

    #[test]
    fn every_role_has_valid_pixel_svg() {
        for &role in ALL_ROLES {
            let svg = get_icon_svg(role, "pixel")
                .unwrap_or_else(|| panic!("missing pixel SVG for {role}"));
            assert!(svg.starts_with("<svg"), "{role} is not an svg: {svg}");
            assert!(svg.ends_with("</svg>"), "{role} does not close svg tag");
        }
    }

    #[test]
    fn install_local_icons_creates_files_and_returns_res_paths() {
        let dir = std::env::temp_dir().join(format!("bhippi-test-icons-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);

        let roles = &["heart", "shield", "coin"];
        let installed =
            install_local_icons(&dir, "assets/ui/icons", roles, "clean").expect("install");
        assert_eq!(installed.len(), 3);
        assert_eq!(
            installed.get("heart"),
            Some(&"res://assets/ui/icons/heart.svg".to_owned())
        );
        assert!(dir.join("assets/ui/icons/heart.svg").is_file());
        assert!(dir.join("assets/ui/icons/shield.svg").is_file());
        assert!(dir.join("assets/ui/icons/coin.svg").is_file());

        let _ = std::fs::remove_dir_all(&dir);
    }
}
