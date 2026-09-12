//! A selected model is inspected by Godot in an isolated, embedded viewer (ADR-0069).
use crate::{
    commands::AppError,
    godot::{self, GodotProcessHandle},
    godot_embed::{parent_window, win, ViewportRect},
};
use bhippi_engine::godot::{
    command::CommandSpec,
    scaffold::{self, ProjectTemplate},
};
use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex, MutexGuard},
    time::{Duration, Instant},
};

#[derive(Default)]
struct Preview {
    id: String,
    process: Option<GodotProcessHandle>,
    hwnd: Option<isize>,
    rect: ViewportRect,
    visible: bool,
    error: Option<String>,
}

#[derive(Clone, Default)]
pub struct AssetPreviewHost(Arc<Mutex<Preview>>);

impl AssetPreviewHost {
    fn lock(&self) -> MutexGuard<'_, Preview> {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
    fn stop(preview: &mut Preview) {
        if let Some(hwnd) = preview.hwnd.take() {
            win::set_visible(hwnd, false);
        }
        if let Some(process) = preview.process.take() {
            process.kill_now();
        }
        preview.id.clear();
    }
    pub fn shutdown(&self) {
        Self::stop(&mut self.lock());
    }
    fn begin(&self, id: String) -> (GodotProcessHandle, godot::GodotStopSignal) {
        let mut held = self.lock();
        Self::stop(&mut held);
        let (process, signal) = godot::stop_channel();
        *held = Preview {
            id,
            process: Some(process.clone()),
            ..Preview::default()
        };
        (process, signal)
    }
    fn close(&self, id: &str) {
        let mut held = self.lock();
        if held.id == id {
            Self::stop(&mut held);
        }
    }
    fn current(&self, id: &str) -> bool {
        self.lock().id == id
    }
}

pub(crate) fn viewer_command(exe: &Path, scratch: &Path, model: &Path) -> CommandSpec {
    CommandSpec {
        program: exe.to_owned(),
        args: vec![
            "--path".into(),
            scratch.to_string_lossy().into_owned(),
            "--script".into(),
            scratch.join("viewer.gd").to_string_lossy().into_owned(),
            "--rendering-method".into(),
            "gl_compatibility".into(),
            "--resolution".into(),
            "800x600".into(),
            "--position".into(),
            "-10000,-10000".into(),
            "--".into(),
            model.to_string_lossy().into_owned(),
        ],
        cwd: Some(scratch.to_owned()),
        env: Vec::new(),
        timeout_secs: 0,
    }
}

async fn prepare_folder() -> Result<PathBuf, AppError> {
    let folder = std::env::temp_dir().join(format!("bhippi-asset-preview-{}", ulid::Ulid::new()));
    let target = folder.clone();
    tokio::task::spawn_blocking(move || {
        scaffold::write_project(&target, "Asset preview", ProjectTemplate::Empty3D, false)
    })
    .await
    .map_err(|error| AppError::plain(error.to_string()))?
    .map_err(|error| AppError::plain(error.to_string()))?;
    tokio::fs::write(folder.join("viewer.gd"), include_str!("asset_viewer.gd"))
        .await
        .map_err(|error| AppError::plain(error.to_string()))?;
    Ok(folder)
}

async fn converted_model(
    source: &Path,
    folder: &Path,
    blender_path: Option<&str>,
    stop: godot::GodotStopSignal,
) -> Result<PathBuf, AppError> {
    let extension = source
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if matches!(extension.as_str(), "glb" | "gltf") {
        return Ok(source.to_owned());
    }
    let blender = crate::blender::locate(blender_path).await?;
    let output = folder.join("preview.glb");
    let script = folder.join("convert.py");
    tokio::fs::write(&script, include_str!("asset_convert.py"))
        .await
        .map_err(|e| AppError::plain(e.to_string()))?;
    tokio::fs::write(
        folder.join("conversion.json"),
        serde_json::json!({"source":source,"destination":output}).to_string(),
    )
    .await
    .map_err(|e| AppError::plain(e.to_string()))?;
    let mut spec = bhippi_engine::blender::script_command(&blender.exe, &script, None);
    spec.args.splice(
        0..0,
        [
            "--factory-startup".into(),
            "--disable-autoexec".into(),
            "--python-exit-code".into(),
            "1".into(),
        ],
    );
    let mut log = String::new();
    let exit = godot::run_spec_with_stop(&spec, Some(stop), |line| {
        tracing::info!(output = %line.text, "asset conversion");
        if log.len() < 16_384 {
            log.push_str(&line.text);
            log.push('\n');
        }
    })
    .await?;
    if !exit.is_success() || !output.is_file() {
        return Err(AppError::new(
            format!("Could not convert this model for preview. {log}"),
            "Try exporting GLB or glTF from the source application.",
        ));
    }
    Ok(output)
}

#[tauri::command]
#[specta::specta]
pub async fn asset_preview_open(
    app: tauri::AppHandle,
    state: tauri::State<'_, crate::Runtime>,
    host: tauri::State<'_, AssetPreviewHost>,
    sessions: tauri::State<'_, crate::godot_commands::GodotSessionStore>,
    id: String,
    project_path: String,
    relative: String,
) -> Result<(), AppError> {
    let parent = parent_window(&app)?;
    let (process, stop) = host.begin(id.clone());
    let result = async {
        let (root, source) = crate::files::resolve(&state, &relative).await?;
        let expected = tokio::fs::canonicalize(project_path)
            .await
            .map_err(|e| AppError::plain(e.to_string()))?;
        if root != expected {
            return Err(AppError::plain(
                "The active project changed. Select the model again.",
            ));
        }
        let extension = source
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or_default();
        if crate::files::preview_type(extension).0 != crate::files::FilePreviewKind::Model {
            return Err(AppError::plain("This file is not a supported 3D model."));
        }
        if tokio::fs::metadata(&source)
            .await
            .map_err(|e| AppError::plain(e.to_string()))?
            .len()
            > bhippi_types::WORKSPACE_MODEL_MAX_BYTES
        {
            return Err(AppError::plain(
                "This model is too large for the preview. Export a smaller GLB.",
            ));
        }
        let install =
            crate::godot_commands::require_install(&state, &sessions, &root.to_string_lossy())
                .await?;
        let blender_path = state
            .config
            .load()
            .await
            .ok()
            .and_then(|cfg| cfg.mcp.blender.blender_path);
        if !host.current(&id) {
            return Err(AppError::plain("Preview selection changed."));
        }
        let scratch = prepare_folder().await?;
        if !host.current(&id) {
            let _removed = tokio::fs::remove_dir_all(&scratch).await;
            return Err(AppError::plain("Preview selection changed."));
        }
        let prepared =
            converted_model(&source, &scratch, blender_path.as_deref(), stop.clone()).await;
        let model = match prepared {
            Ok(model) if host.current(&id) => model,
            other => {
                let _removed = tokio::fs::remove_dir_all(&scratch).await;
                return Err(other
                    .err()
                    .unwrap_or_else(|| AppError::plain("Preview selection changed.")));
            }
        };
        let spec = viewer_command(install.gui(), &scratch, &model);
        let running = host.inner().clone();
        let running_id = id.clone();
        let (ready_tx, ready_rx) = tokio::sync::watch::channel(false);
        let runner = tokio::spawn(async move {
            let output_host = running.clone();
            let output_id = running_id.clone();
            let exit = godot::run_spec_with_stop(&spec, Some(stop), move |line| {
                tracing::info!(output = %line.text, "asset viewer");
                if line.text.contains("BHIPPI_ASSET_READY") {
                    let _sent = ready_tx.send(true);
                }
                if let Some(error) = line.text.strip_prefix("BHIPPI_ASSET_ERROR: ") {
                    let mut held = output_host.lock();
                    if held.id == output_id {
                        held.error = Some(error.to_owned());
                    }
                }
            })
            .await;
            {
                let mut held = running.lock();
                if held.id == running_id {
                    held.hwnd = None;
                    if held.error.is_none() {
                        held.error = Some(match exit {
                            Err(error) => error.message,
                            _ => "The model preview closed. Select Retry to reopen it.".into(),
                        });
                    }
                }
            }
            let _removed = tokio::fs::remove_dir_all(scratch).await;
        });
        let deadline =
            Instant::now() + Duration::from_secs(bhippi_types::ASSET_PREVIEW_START_TIMEOUT_SECS);
        let mut attached = false;
        loop {
            if !host.current(&id) {
                process.kill_now();
                return Err(AppError::plain("Preview selection changed."));
            }
            if let Some(error) = host.lock().error.clone() {
                process.kill_now();
                return Err(AppError::plain(error));
            }
            if runner.is_finished() || Instant::now() >= deadline {
                process.kill_now();
                return Err(AppError::new(
                    "The model preview did not become ready.",
                    "Check the model file and retry.",
                ));
            }
            if !attached {
                if let Some(hwnd) = process.pid().and_then(win::find_engine_window) {
                    let mut held = host.lock();
                    if held.id != id {
                        continue;
                    }
                    win::adopt(hwnd, parent.hwnd, held.rect.to_physical(parent.scale))
                        .map_err(AppError::plain)?;
                    win::set_visible(hwnd, held.visible && !held.rect.is_empty());
                    held.hwnd = Some(hwnd);
                    attached = true;
                }
            }
            if attached && *ready_rx.borrow() {
                return Ok(());
            }
            tokio::time::sleep(Duration::from_millis(16)).await;
        }
    }
    .await;
    if let Err(error) = &result {
        process.kill_now();
        let mut held = host.lock();
        if held.id == id {
            held.error = Some(error.message.clone());
        }
    }
    result
}

#[tauri::command]
#[specta::specta]
pub fn asset_preview_layout(
    app: tauri::AppHandle,
    host: tauri::State<'_, AssetPreviewHost>,
    id: String,
    rect: ViewportRect,
    visible: bool,
) -> Result<(), AppError> {
    let parent = parent_window(&app)?;
    let mut held = host.lock();
    if held.id != id {
        return Ok(());
    }
    held.rect = rect;
    held.visible = visible && !rect.is_empty();
    if let Some(hwnd) = held.hwnd {
        if held.visible {
            win::place(hwnd, rect.to_physical(parent.scale));
        } else {
            win::set_visible(hwnd, false);
        }
    }
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn asset_preview_close(host: tauri::State<'_, AssetPreviewHost>, id: String) {
    host.close(&id);
}

#[tauri::command]
#[specta::specta]
pub fn asset_preview_status(
    host: tauri::State<'_, AssetPreviewHost>,
    id: String,
) -> Option<String> {
    let held = host.lock();
    if held.id == id {
        held.error.clone()
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stale_close_cannot_stop_a_newer_preview_and_replacement_stops_the_old_one() {
        let host = AssetPreviewHost::default();
        let (old, _old_signal) = host.begin("old".into());
        let (current, _current_signal) = host.begin("current".into());
        assert!(old.is_stopped());
        host.close("old");
        assert!(!current.is_stopped());
        assert!(host.current("current"));
        host.close("current");
        assert!(current.is_stopped());
        assert!(!host.current("current"));
    }

    #[test]
    fn viewer_command_preserves_paths_and_never_opens_the_project_editor() {
        let spec = viewer_command(
            Path::new("engine.exe"),
            Path::new("C:/preview folder"),
            Path::new("C:/project/models/large model.glb"),
        );
        assert_eq!(
            spec.args.last().unwrap(),
            "C:/project/models/large model.glb"
        );
        assert!(!spec.args.iter().any(|arg| arg == "--editor"));
        assert!(spec
            .args
            .windows(2)
            .any(|pair| pair == ["--position", "-10000,-10000"]));
        assert_eq!(spec.timeout_secs, 0);
    }

    #[tokio::test]
    async fn direct_gltf_preview_does_not_require_blender_or_modify_the_source() {
        let (_, stop) = godot::stop_channel();
        for name in ["model.glb", "model.gltf", "model.GLB"] {
            let source = Path::new(name);
            assert_eq!(
                converted_model(
                    source,
                    Path::new("unused"),
                    Some("missing-blender"),
                    stop.clone()
                )
                .await
                .unwrap(),
                source
            );
        }
    }

    #[tokio::test]
    #[ignore = "requires installed Blender and a graphical Godot; renders an isolated fixture"]
    async fn live_asset_preview_converts_renders_and_navigates() {
        let bundled = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("resources/godot/Godot_v4.7.1-stable_win64.exe");
        let install = godot::detect_godot(Some(&bundled))
            .await
            .expect("Godot is required");
        let scratch = prepare_folder().await.unwrap();
        let source = scratch.join("tetrahedron.obj");
        let original = "mtllib colors.mtl\no Tetrahedron\nv 0 1 0\nv -1 -1 1\nv 1 -1 1\nv 0 -1 -1\nusemtl Terracotta\nf 1 2 3\nf 1 3 4\nf 1 4 2\nf 2 4 3\n";
        tokio::fs::write(&source, original).await.unwrap();
        tokio::fs::write(
            scratch.join("colors.mtl"),
            "newmtl Terracotta\nKd 0.7 0.2 0.08\n",
        )
        .await
        .unwrap();
        let (process, stop) = godot::stop_channel();
        let model = converted_model(&source, &scratch, None, stop.clone())
            .await
            .unwrap();
        assert_eq!(tokio::fs::read_to_string(&source).await.unwrap(), original);
        assert_eq!(&tokio::fs::read(&model).await.unwrap()[..4], b"glTF");
        tokio::fs::write(
            scratch.join("check.gd"),
            include_str!("../tests/fixtures/asset-viewer-check.gd"),
        )
        .await
        .unwrap();
        let screenshot = scratch.join("preview.png");
        let mut spec = viewer_command(install.gui(), &scratch, &model);
        let script_index = spec.args.iter().position(|arg| arg == "--script").unwrap() + 1;
        spec.args[script_index] = scratch.join("check.gd").to_string_lossy().into_owned();
        spec.args.push(screenshot.to_string_lossy().into_owned());
        spec.timeout_secs = 45;
        let mut output = String::new();
        let exit = godot::run_spec_with_stop(&spec, Some(stop), |line| {
            output.push_str(&line.text);
            output.push('\n');
        })
        .await
        .unwrap();
        process.kill_now();
        assert!(exit.is_success(), "{output}");
        assert!(!output.contains("SCRIPT ERROR"), "{output}");
        assert!(output.contains("BHIPPI_ASSET_READY"), "{output}");
        assert!(output.contains("BHIPPI_ASSET_CONTROLS_OK"), "{output}");
        assert!(screenshot.metadata().unwrap().len() > 1_000);
        println!("Asset preview render: {}", screenshot.display());
    }
}
