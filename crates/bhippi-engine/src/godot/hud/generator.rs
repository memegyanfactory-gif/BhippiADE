//! Autonomous AI HUD Asset Generator and Library Extension (INV-074, GAD-160).
//!
//! Provides a safe, extensible fallback system that synthesizes new HUD icons, boxes,
//! and panel components on the fly when an AI agent or user specifies game mechanics
//! beyond the pre-existing built-in catalogue.
//!
//! Generated assets are fully compliant with Godot 4 SVG rasterization and carry explicit
//! CC0 attribution sidecars so that the project exports cleanly under release gates.

use crate::error::{EngineError, Result};
use std::path::Path;

/// Synthesizes a valid, clean SVG icon for any given role keyword and style.
#[must_use]
pub fn synthesize_icon_svg(role: &str, style: &str) -> String {
    let is_pixel = style.eq_ignore_ascii_case("pixel") || style.eq_ignore_ascii_case("retro");
    let clean_role = role.trim().to_ascii_lowercase();

    if is_pixel {
        synthesize_pixel_icon(&clean_role)
    } else {
        synthesize_clean_icon(&clean_role)
    }
}

fn synthesize_clean_icon(role: &str) -> String {
    match role {
        "battery" | "energy" => r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 32 32" width="32" height="32">
  <rect x="7" y="6" width="18" height="22" rx="3" fill="#37474f"/>
  <rect x="12" y="2" width="8" height="4" rx="1" fill="#78909c"/>
  <rect x="10" y="10" width="12" height="14" rx="1" fill="#00e676"/>
  <polygon points="17,11 13,18 16,18 15,23 19,16 16,16" fill="#ffffff"/>
</svg>"##
        .to_string(),
        "laser" | "blaster" => r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 32 32" width="32" height="32">
  <line x1="4" y1="16" x2="28" y2="16" stroke="#ff1744" stroke-width="4" stroke-linecap="round"/>
  <line x1="8" y1="16" x2="24" y2="16" stroke="#ffffff" stroke-width="2" stroke-linecap="round"/>
  <circle cx="28" cy="16" r="3" fill="#ff5252"/>
</svg>"##
        .to_string(),
        "chest" | "crate" => r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 32 32" width="32" height="32">
  <rect x="4" y="8" width="24" height="20" rx="3" fill="#8d6e63" stroke="#ffd54f" stroke-width="2"/>
  <rect x="4" y="6" width="24" height="8" rx="2" fill="#a1887f" stroke="#ffd54f" stroke-width="2"/>
  <circle cx="16" cy="18" r="3" fill="#ffd54f"/>
  <rect x="15" y="18" width="2" height="4" fill="#3e2723"/>
</svg>"##
        .to_string(),
        "wrench" | "repair" => r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 32 32" width="32" height="32">
  <path fill="#90a4ae" d="M26 6a8 8 0 0 0-10 2l3 3-3 3-3-3a8 8 0 0 0-2 10l12 12a3 3 0 0 0 4-4L15 17a8 8 0 0 0 11-11z"/>
  <circle cx="23" cy="9" r="2" fill="#37474f"/>
</svg>"##
        .to_string(),
        "flame" | "fire" => r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 32 32" width="32" height="32">
  <path fill="#ff3d00" d="M16 2c0 6-7 9-7 16a9 9 0 0 0 18 0c0-7-7-10-7-16z"/>
  <path fill="#ffea00" d="M16 11c0 3-4 5-4 9a5 5 0 0 0 10 0c0-4-4-6-4-9z"/>
</svg>"##
        .to_string(),
        "crown" | "king" => r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 32 32" width="32" height="32">
  <polygon points="4,24 28,24 28,8 21,16 16,6 11,16 4,8" fill="#ffd700" stroke="#ffb300" stroke-width="1.5"/>
  <rect x="4" y="24" width="24" height="4" rx="1" fill="#ffa000"/>
  <circle cx="16" cy="6" r="2" fill="#e91e63"/>
  <circle cx="4" cy="8" r="1.5" fill="#00e5ff"/>
  <circle cx="28" cy="8" r="1.5" fill="#00e5ff"/>
</svg>"##
        .to_string(),
        "portal" | "warp" => r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 32 32" width="32" height="32">
  <ellipse cx="16" cy="16" rx="12" ry="14" fill="none" stroke="#7c4dff" stroke-width="3"/>
  <ellipse cx="16" cy="16" rx="8" ry="10" fill="none" stroke="#00e5ff" stroke-width="2.5"/>
  <ellipse cx="16" cy="16" rx="4" ry="5" fill="#ffffff" opacity="0.8"/>
</svg>"##
        .to_string(),
        // Fallback: Elegant heraldic geometric crest with initial letter
        _ => {
            let initial = role.chars().next().unwrap_or('?').to_ascii_uppercase();
            format!(
                r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 32 32" width="32" height="32">
  <polygon points="16,2 28,8 28,24 16,30 4,24 4,8" fill="#2c3440" stroke="#00e5ff" stroke-width="2"/>
  <text x="16" y="21" font-size="14" font-weight="bold" font-family="sans-serif" fill="#ffffff" text-anchor="middle">{initial}</text>
</svg>"##
            )
        }
    }
}

fn synthesize_pixel_icon(role: &str) -> String {
    let initial = role.chars().next().unwrap_or('?').to_ascii_uppercase();
    format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16" width="32" height="32" shape-rendering="crispEdges">
  <rect x="1" y="1" width="14" height="14" fill="#21252b" stroke="#ffeb3b" stroke-width="1"/>
  <text x="8" y="12" font-size="10" font-weight="bold" font-family="monospace" fill="#ffffff" text-anchor="middle">{initial}</text>
</svg>"##
    )
}

/// Synthesizes a custom HUD box container SVG (e.g. reward card, kill feed box, overhead health).
#[must_use]
pub fn synthesize_box_svg(box_type: &str, accent_hex: &str, plate_hex: &str) -> String {
    match box_type {
        "kill_feed" => format!(
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 160 32" width="160" height="32">
  <rect x="0" y="0" width="160" height="32" rx="4" fill="{plate_hex}" fill-opacity="0.85"/>
  <line x1="0" y1="0" x2="4" y2="32" stroke="{accent_hex}" stroke-width="4"/>
</svg>"##
        ),
        "reward_card" => format!(
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 80 96" width="80" height="96">
  <rect x="2" y="4" width="76" height="90" rx="8" fill="#000000" fill-opacity="0.4"/>
  <rect x="2" y="2" width="76" height="90" rx="8" fill="{plate_hex}" stroke="{accent_hex}" stroke-width="2"/>
  <rect x="6" y="6" width="68" height="40" rx="4" fill="#ffffff" fill-opacity="0.08"/>
</svg>"##
        ),
        _ => format!(
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 120 40" width="120" height="40">
  <rect x="1" y="1" width="118" height="38" rx="6" fill="{plate_hex}" stroke="{accent_hex}" stroke-width="2"/>
</svg>"##
        ),
    }
}

/// Synthesize and install a custom icon into the project's `assets/ui/icons/` folder.
pub fn register_and_install_custom_icon(
    project_root: &Path,
    role: &str,
    style: &str,
) -> Result<String> {
    let icons_dir = project_root.join("assets").join("ui").join("icons");
    std::fs::create_dir_all(&icons_dir).map_err(|err| EngineError::Io {
        operation: "install_custom_icon",
        path: icons_dir.display().to_string(),
        reason: err.to_string(),
        hint: Some("Check folder permissions.".to_owned()),
    })?;

    let file_name = format!("{role}.svg");
    let file_path = icons_dir.join(&file_name);
    if !file_path.exists() {
        let svg = synthesize_icon_svg(role, style);
        std::fs::write(&file_path, svg).map_err(|err| EngineError::Io {
            operation: "write_custom_icon",
            path: file_path.display().to_string(),
            reason: err.to_string(),
            hint: None,
        })?;
    }

    // Sidecar metadata
    let sidecar_path = icons_dir.join(format!("{role}.svg.meta.json"));
    if !sidecar_path.exists() {
        let sidecar = serde_json::json!({
            "role": role,
            "style": style,
            "license": "CC0-1.0",
            "author": "Bhippi AI Generator (CC0 1.0 Universal)",
            "provenance": "autonomous_hud_generator"
        });
        std::fs::write(
            &sidecar_path,
            serde_json::to_string_pretty(&sidecar).map_err(|e| {
                EngineError::Manifest(format!("failed to serialise icon sidecar: {e}"), None)
            })?,
        )
        .map_err(|err| EngineError::Io {
            operation: "write_custom_icon_sidecar",
            path: sidecar_path.display().to_string(),
            reason: err.to_string(),
            hint: None,
        })?;
    }

    Ok(format!("res://assets/ui/icons/{file_name}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_synthesis_produces_valid_svg_for_known_and_novel_roles() {
        for role in &["battery", "wrench", "portal", "custom_shield", "plasma_orb"] {
            let svg = synthesize_icon_svg(role, "clean");
            assert!(svg.starts_with("<svg"), "{role} missing svg open tag");
            assert!(svg.ends_with("</svg>"), "{role} missing svg close tag");
        }
    }

    #[test]
    fn pixel_synthesis_produces_crisp_pixel_art_svg() {
        let svg = synthesize_icon_svg("gem_shard", "pixel");
        assert!(svg.contains("shape-rendering=\"crispEdges\""));
        assert!(svg.ends_with("</svg>"));
    }

    #[test]
    fn custom_box_synthesis_produces_valid_svg() {
        let kill_feed = synthesize_box_svg("kill_feed", "#ff3d00", "#141a24");
        assert!(kill_feed.contains("<rect"));
        assert!(kill_feed.contains("</svg>"));

        let reward_card = synthesize_box_svg("reward_card", "#ffd700", "#1e2430");
        assert!(reward_card.contains("</svg>"));
    }

    #[test]
    fn register_and_install_custom_icon_writes_files_and_sidecars() {
        let dir = std::env::temp_dir().join(format!("bhippi-gen-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);

        let path = register_and_install_custom_icon(&dir, "hyper_drive", "clean").expect("install");
        assert_eq!(path, "res://assets/ui/icons/hyper_drive.svg");

        let icon_file = dir.join("assets/ui/icons/hyper_drive.svg");
        assert!(icon_file.is_file());
        let sidecar_file = dir.join("assets/ui/icons/hyper_drive.svg.meta.json");
        assert!(sidecar_file.is_file());

        let _ = std::fs::remove_dir_all(&dir);
    }
}
