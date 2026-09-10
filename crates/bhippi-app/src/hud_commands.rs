//! The IPC surface over the HUD library and the external asset library (GAD-160).
//!
//! Two things live here, and they are one feature: a HUD is a set of readouts, and a readout
//! wants an icon. [`hud_apply`] builds the HUD; [`fab_import_icons`] is where the
//! icons come from when the project has none of its own.
//!
//! Everything a HUD build writes goes through the same path as every other Godot edit —
//! `lower` → `apply_changeset` → `--check-only` → the journal — so a HUD is undoable, and a
//! generated script that does not compile leaves the project exactly as it found it. There is
//! no second writer here.

use crate::commands::AppError;
use crate::godot_commands::{
    apply_batch_for, engine_error, normalise_actor, resolve_project, GodotApplyHost,
};
use bhippi_engine::fab::{self, FabPack, IconImport, IconRequest};
use bhippi_engine::godot::hud::{self, HudBuildOptions, HudLibraryView, HudProjectState};
use serde::{Deserialize, Serialize};
use specta::Type;
use std::path::PathBuf;

/// What one HUD build did, plus what it could not find art for.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct HudApplyResult {
    pub preset: String,
    pub skin: String,
    /// Project-relative files the build wrote.
    pub files: Vec<String>,
    /// Icon roles the preset wanted and the project had nothing for. The HUD falls back to
    /// its captions for these, so it is a note rather than a failure — and it is the exact
    /// list [`fab_import_icons`] should be asked for next.
    pub unresolved_icons: Vec<String>,
    /// The journal revision the change landed on, when the ledger was available.
    pub revision: Option<i64>,
    /// True when this replaced a HUD the project already had.
    pub replaced: bool,
}

/// The Fab vault as the picker sees it.
///
/// Named apart from `asset_library::AssetLibraryView` on purpose: that one is the user's own
/// registered folders, which Bhippi reads directly. This one is a vendor cache full of
/// formats — `.uasset`, `.unitypackage` — that have to be classified before they mean
/// anything to Godot.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct FabVaultView {
    /// The folder that was read, or empty when none was found.
    pub root: String,
    /// True when the default Fab vault exists on this machine.
    pub found: bool,
    pub packs: Vec<FabPack>,
    /// One line for when nothing usable is there, so an empty picker explains itself.
    pub note: String,
}

/// Every HUD preset, skin and icon role.
///
/// `archetype` orders the presets so the HUD that fits the game leads; an empty string keeps
/// the library order.
#[tauri::command]
#[specta::specta]
pub fn hud_library(archetype: String) -> HudLibraryView {
    hud::library_for(archetype.trim())
}

/// What HUD a project currently carries, read from the project rather than from a manifest.
#[tauri::command]
#[specta::specta]
pub async fn hud_project_state(
    state: tauri::State<'_, crate::Runtime>,
    project: String,
) -> Result<HudProjectState, AppError> {
    let root = resolve_project(&state, &project).await?;
    Ok(hud::project_state(&root))
}

/// Build a HUD into a project.
///
/// The two facts a rebuild depends on — is there a HUD scene already, is there an instance in
/// the main scene — are read off disk by [`HudBuildOptions::for_project`] rather than guessed,
/// which is what makes changing preset or skin an ordinary edit instead of a refusal.
#[tauri::command]
#[specta::specta]
pub async fn hud_apply(
    app: tauri::AppHandle,
    state: tauri::State<'_, crate::Runtime>,
    project: String,
    preset: String,
    skin: String,
    actor: String,
) -> Result<HudApplyResult, AppError> {
    let root = resolve_project(&state, &project).await?;
    let mut options = HudBuildOptions::for_project(&root, preset.trim());
    let replaced = options.replace_scene || options.detach_existing;
    if !skin.trim().is_empty() {
        options = options.with_skin(skin.trim());
    }

    let built = hud::build(&options).map_err(engine_error)?;
    let outcome = apply_batch_for(
        GodotApplyHost { app: Some(&app) },
        &root,
        &built.batch,
        normalise_actor(&actor)?.as_str(),
    )
    .await
    .map_err(|failure| failure.error)?;

    Ok(HudApplyResult {
        preset: built.preset.id.to_owned(),
        skin: built.skin.id.to_owned(),
        files: built.files,
        unresolved_icons: built.unresolved_icons,
        revision: outcome.revision,
        replaced,
    })
}

/// Read the external asset library.
///
/// `vault` overrides the machine default. A pack Godot cannot open is *listed with its
/// reason* rather than hidden: an Unreal-only entry that silently vanishes reads as a bug in
/// Bhippi, and the useful answer is that the pack was never downloaded in a format Godot has.
#[tauri::command]
#[specta::specta]
pub fn fab_vault_scan(vault: String) -> Result<FabVaultView, AppError> {
    let trimmed = vault.trim();
    let root = if trimmed.is_empty() {
        fab::default_vault()
    } else {
        let path = PathBuf::from(trimmed);
        path.is_dir().then_some(path)
    };

    let Some(root) = root else {
        return Ok(FabVaultView {
            root: String::new(),
            found: false,
            packs: Vec::new(),
            note: "No asset library found. Install a pack through the Epic Games Launcher, or \
                   point Bhippi at a folder of packs."
                .to_owned(),
        });
    };

    let packs = fab::scan(&root).map_err(engine_error)?;
    let usable = packs.iter().filter(|pack| pack.supplies_icons).count();
    let note = if packs.is_empty() {
        "That folder has no asset packs in it.".to_owned()
    } else if usable == 0 {
        format!(
            "{} packs, none of which carry 2D art a HUD can use. The models are still \
             importable for the game itself.",
            packs.len()
        )
    } else {
        format!(
            "{} packs, {usable} of which can supply HUD icons.",
            packs.len()
        )
    };

    Ok(FabVaultView {
        root: root.display().to_string(),
        found: true,
        packs,
        note,
    })
}

/// Pull the icons a HUD preset asked for out of one pack and into the project.
///
/// `license` is required and is written into a `.meta.json` beside every file. The Fab
/// metadata records a title, a seller and a category but no terms, so Bhippi cannot know
/// them — and INV-074 refuses to export an asset whose sidecar cannot name its licence.
/// Asking here, while the user is looking at the pack, is the honest moment.
#[tauri::command]
#[specta::specta]
pub async fn fab_import_icons(
    state: tauri::State<'_, crate::Runtime>,
    project: String,
    vault: String,
    pack_id: String,
    preset: String,
    license: String,
) -> Result<IconImport, AppError> {
    let root = resolve_project(&state, &project).await?;
    let view = fab_vault_scan(vault)?;
    let pack = view
        .packs
        .into_iter()
        .find(|pack| pack.id == pack_id.trim())
        .ok_or_else(|| AppError {
            message: format!("No asset pack called {pack_id}."),
            hint: Some("Rescan the library; the pack may have been moved or removed.".to_owned()),
        })?;

    let requests = icon_requests(preset.trim(), &root);
    if requests.is_empty() {
        return Ok(IconImport {
            imported: Vec::new(),
            unmatched: Vec::new(),
            license: license.trim().to_owned(),
        });
    }

    fab::import_icons(&pack, &root, hud::HUD_ICON_DIR, &requests, license.trim())
        .map_err(engine_error)
}

/// The roles worth asking a pack for: the ones the preset names and the project does not
/// already answer. Re-importing an icon a project already has would overwrite art the user
/// may have replaced by hand.
fn icon_requests(preset_id: &str, project_root: &std::path::Path) -> Vec<IconRequest> {
    let roles: Vec<&'static hud::IconRole> = match hud::preset(preset_id) {
        Some(entry) => hud::roles_for(entry),
        None => hud::icon_roles().iter().collect(),
    };
    let existing = hud::preset(preset_id)
        .map(|entry| hud::icons_from_project(project_root, entry))
        .unwrap_or_default();

    roles
        .into_iter()
        .filter(|role| !existing.contains_key(role.id))
        .map(|role| IconRequest {
            role: role.id.to_owned(),
            keywords: role
                .keywords
                .iter()
                .map(|word| (*word).to_owned())
                .collect(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_scan_of_nowhere_explains_itself_rather_than_failing() {
        let view = fab_vault_scan("Z:/no/such/vault".to_owned()).expect("it answers");
        assert!(!view.found);
        assert!(view.packs.is_empty());
        assert!(view.note.contains("No asset library"), "{}", view.note);
    }

    #[test]
    fn only_the_roles_a_project_lacks_are_requested() {
        let dir = std::env::temp_dir().join(format!("bhippi-hud-req-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join(hud::HUD_ICON_DIR)).expect("icon dir");

        let all = icon_requests("preset.hud.lives_score", &dir);
        assert!(all.iter().any(|request| request.role == "heart"));
        assert!(all.iter().any(|request| request.role == "coin"));

        std::fs::write(dir.join(hud::HUD_ICON_DIR).join("heart.png"), b"x").expect("icon");
        let rest = icon_requests("preset.hud.lives_score", &dir);
        assert!(
            !rest.iter().any(|request| request.role == "heart"),
            "art the project already has must not be overwritten"
        );
        assert!(rest.iter().any(|request| request.role == "coin"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_library_orders_a_shooters_hud_first_for_a_shooter() {
        let view = hud_library("fps_arena".to_owned());
        assert_eq!(view.presets[0].id, "preset.hud.ammo_health");
        assert_eq!(view.presets.len(), hud::presets().len());
        assert!(view
            .presets
            .iter()
            .all(|entry| entry.persistent <= view.max_persistent));
    }
}
