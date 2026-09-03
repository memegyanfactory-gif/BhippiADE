//! The studio viewport **is** Godot (ADR-0045).
//!
//! There is no picture of the project in the webview — no Three.js scene, no screenshot
//! stream, no export in an iframe standing in for the editor. The viewport card is a hole in
//! the page, and the real Godot window sits in that hole: the editor when the workspace is
//! open, the running game on top of it while Play is pressed. What the user sees is what
//! Godot draws, at Godot's frame rate, with no copy in between — identical by construction.
//!
//! The mechanism is the one Godot's own editor uses for its embedded Game view on Windows:
//! the engine process is spawned as usual (`godot.rs`, argv-only, scrubbed env), its
//! top-level window is found by pid, and that window is re-parented into Bhippi's window as
//! a `WS_CHILD` and placed over the viewport rect. The webview reports the rect in CSS pixels
//! whenever it changes; Rust converts it with the window's scale factor and moves the child.
//! No PowerShell bridge on this path — a resize must land in the same frame, so every call
//! here is a direct Win32 call.
//!
//! Rules this module keeps (INV-090):
//! - Every Godot surface the studio opens is embedded here. A Godot window that is not a
//!   child of Bhippi's window is a bug, not a fallback.
//! - Nothing in the webview may overlay the viewport rect: the page cannot paint on top of a
//!   native child. When a modal opens, the page tells this module and the child is hidden.
//! - With nothing running, the viewport is empty. This module never draws a placeholder.
//! - Off Windows there is no embedding. The commands fail with a typed, actionable error and
//!   the pane shows that error; they never open a stray window instead.

use crate::commands::AppError;
use crate::godot::{run_spec_observed, stop_channel, GodotProcessHandle};
use crate::godot_commands::{
    announce_process, claim_slot, display_of, lock as lock_sessions, release_slot, require_install,
    resolve_project, start_output_pump, GodotRunKind, GodotRunState, GodotSessionStore,
};
use bhippi_engine::godot::command::{editor_command, run_command, RunOptions};
use bhippi_engine::godot::detect::GodotInstall;
use serde::{Deserialize, Serialize};
use specta::Type;
use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};
use tauri::Manager;
use tauri_specta::Event;

/// The Tauri window the viewport lives in.
const MAIN_WINDOW: &str = "main";

/// How long the engine gets to show its window after the process starts. The first open of
/// a project imports every asset before the editor appears, which is slow on purpose.
const WINDOW_WAIT: Duration = Duration::from_secs(90);
/// How often the window is looked for while waiting.
const WINDOW_POLL: Duration = Duration::from_millis(16);
/// A game asked to close gets this long to do it before it is killed. The editor is never
/// killed on a close request: it may be asking whether to save.
const GAME_CLOSE_GRACE: Duration = Duration::from_millis(2_500);
/// The backstop that keeps the child on top of the webview between layout calls.
const KEEPER_TICK: Duration = Duration::from_millis(250);
/// A surface standing in the way of a project switch gets this long to close itself before
/// it is killed. There is one workspace slot and one game slot for the whole app, so the
/// outgoing engine has to be gone before the next one is spawned — two Godots racing over
/// two projects in one hole is a stray window and a re-import, whichever wins.
const SWITCH_CLOSE_GRACE: Duration = Duration::from_secs(5);
/// How often the switch looks to see whether the outgoing surface has gone.
const SWITCH_POLL: Duration = Duration::from_millis(50);

// ── the rect ─────────────────────────────────────────────────────────────────────────

/// The viewport's box in the webview's CSS pixels, as `getBoundingClientRect` reports it.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Serialize, Type)]
pub struct ViewportRect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// The same box in the parent window's physical client pixels — what `SetWindowPos` takes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PhysicalRect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

impl ViewportRect {
    /// CSS pixels to physical pixels. The edges are rounded independently so the child never
    /// drifts a pixel from the pane beside it as the scale changes; the size never drops
    /// below 1×1, because Windows treats a zero-sized child as gone and Godot stops drawing.
    #[must_use]
    pub fn to_physical(self, scale: f64) -> PhysicalRect {
        let scale = if scale.is_finite() && scale > 0.0 {
            scale
        } else {
            1.0
        };
        let left = (self.x * scale).round();
        let top = (self.y * scale).round();
        let right = ((self.x + self.width) * scale).round();
        let bottom = ((self.y + self.height) * scale).round();
        PhysicalRect {
            x: clamp_i32(left),
            y: clamp_i32(top),
            width: clamp_i32((right - left).max(1.0)),
            height: clamp_i32((bottom - top).max(1.0)),
        }
    }

    #[must_use]
    pub fn is_empty(self) -> bool {
        !(self.width > 0.0 && self.height > 0.0)
    }
}

fn clamp_i32(value: f64) -> i32 {
    value.clamp(f64::from(i32::MIN), f64::from(i32::MAX)) as i32
}

// ── state ────────────────────────────────────────────────────────────────────────────

/// Which Godot surface a window is.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum EmbedSurface {
    /// The Godot editor on the project — the workspace.
    Workspace,
    /// The running game. Sits on top of the workspace while it runs.
    Game,
}

impl EmbedSurface {
    #[must_use]
    fn run_kind(self) -> GodotRunKind {
        match self {
            Self::Workspace => GodotRunKind::Editor,
            Self::Game => GodotRunKind::Run,
        }
    }

    #[must_use]
    fn label(self) -> &'static str {
        match self {
            Self::Workspace => "workspace",
            Self::Game => "game",
        }
    }
}

// ── the plan ─────────────────────────────────────────────────────────────────────────

/// What the guards said about the project the user asked for.
///
/// The guards are async — a config read, an engine detection, a file write into the incoming
/// project — so they are asked outside the host lock and their answer arrives here as one of
/// two words. Which guard refused is carried by the typed [`AppError`] the command returns;
/// the plan only needs to know that the launch stops.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Verdict {
    /// Every guard passed: the engine can be spawned.
    Ready,
    /// A guard refused: no `project.godot`, no Godot install, or the project could not be
    /// prepared for the viewport.
    Refused,
}

/// One step of a launch, in the order it happens.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Step {
    /// Close a live surface that belongs to a different project. Always first, and always
    /// taken — a refusal below does not cancel it.
    Close(EmbedSurface),
    /// Stop: hand the page the guard's typed error. The viewport is empty by now.
    Refuse,
    /// Spawn the engine into the hole the closes above cleared.
    Launch,
}

/// One Godot window the viewport owns, as the page sees it.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct EmbeddedWindow {
    pub surface: EmbedSurface,
    pub project: String,
    /// `0` until the process has been spawned.
    pub process_id: u32,
    /// `0` until the window has been found and adopted.
    pub hwnd: u64,
    /// `true` once the window is a child of Bhippi's window.
    pub attached: bool,
}

/// Everything the page needs to draw the viewport's chrome around the hole.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize, Type, Event)]
pub struct GodotEmbedState {
    pub workspace: Option<EmbeddedWindow>,
    pub game: Option<EmbeddedWindow>,
    /// The surface in front: the game while one runs, else the workspace, else nothing.
    pub front: Option<EmbedSurface>,
    /// Whether the viewport is on screen and unobstructed, as the page last said.
    pub visible: bool,
}

#[derive(Debug)]
struct Embedded {
    /// Which launch filled this slot. A project switch kills the outgoing engine and spawns
    /// the next one immediately; the outgoing run task then finishes and must clear *its*
    /// slot, never the one the new launch has already taken.
    token: u64,
    window: EmbeddedWindow,
    handle: GodotProcessHandle,
}

/// The host: the viewport's last known rect and the windows sitting in it. A plain mutex —
/// every critical section is a field read and a handful of Win32 calls, none of them block.
#[derive(Debug, Default)]
pub struct EmbedHost {
    viewport: Option<ViewportRect>,
    visible: bool,
    workspace: Option<Embedded>,
    game: Option<Embedded>,
    /// Monotonic per host; see [`Embedded::token`].
    next_token: u64,
}

impl EmbedHost {
    fn slot(&self, surface: EmbedSurface) -> &Option<Embedded> {
        match surface {
            EmbedSurface::Workspace => &self.workspace,
            EmbedSurface::Game => &self.game,
        }
    }

    fn slot_mut(&mut self, surface: EmbedSurface) -> &mut Option<Embedded> {
        match surface {
            EmbedSurface::Workspace => &mut self.workspace,
            EmbedSurface::Game => &mut self.game,
        }
    }

    /// A surface whose process has already ended is bookkeeping, not a window.
    fn live(&self, surface: EmbedSurface) -> Option<&Embedded> {
        self.slot(surface)
            .as_ref()
            .filter(|embedded| !embedded.handle.is_stopped())
    }

    /// The surface the user should be looking at.
    #[must_use]
    pub fn front(&self) -> Option<EmbedSurface> {
        if self.live(EmbedSurface::Game).is_some() {
            Some(EmbedSurface::Game)
        } else if self.live(EmbedSurface::Workspace).is_some() {
            Some(EmbedSurface::Workspace)
        } else {
            None
        }
    }

    #[must_use]
    pub fn state(&self) -> GodotEmbedState {
        GodotEmbedState {
            workspace: self
                .live(EmbedSurface::Workspace)
                .map(|embedded| embedded.window.clone()),
            game: self
                .live(EmbedSurface::Game)
                .map(|embedded| embedded.window.clone()),
            front: self.front(),
            visible: self.visible,
        }
    }

    /// Take a slot for a new launch. The token identifies this launch for every later write.
    fn begin(&mut self, surface: EmbedSurface, project: &str, handle: GodotProcessHandle) -> u64 {
        self.next_token += 1;
        let token = self.next_token;
        *self.slot_mut(surface) = Some(Embedded {
            token,
            window: EmbeddedWindow {
                surface,
                project: project.to_owned(),
                process_id: 0,
                hwnd: 0,
                attached: false,
            },
            handle,
        });
        token
    }

    /// Whether the slot is still the one this launch took.
    #[must_use]
    fn holds(&self, surface: EmbedSurface, token: u64) -> bool {
        self.slot(surface)
            .as_ref()
            .is_some_and(|embedded| embedded.token == token)
    }

    fn set_pid(&mut self, surface: EmbedSurface, token: u64, pid: u32) {
        if let Some(embedded) = self.slot_mut(surface).as_mut().filter(|e| e.token == token) {
            embedded.window.process_id = pid;
        }
    }

    fn set_attached(&mut self, surface: EmbedSurface, token: u64, hwnd: isize) {
        if let Some(embedded) = self.slot_mut(surface).as_mut().filter(|e| e.token == token) {
            embedded.window.hwnd = hwnd as u64;
            embedded.window.attached = true;
        }
    }

    /// Clear the slot whoever holds it — the foreign-window path, which owns the game slot
    /// outright for the length of a playtest.
    fn end(&mut self, surface: EmbedSurface) {
        *self.slot_mut(surface) = None;
    }

    /// Clear the slot only if this launch still holds it. A run task that ends after a
    /// project switch has already re-filled the slot must leave the new window alone.
    fn end_launch(&mut self, surface: EmbedSurface, token: u64) {
        if self.holds(surface, token) {
            *self.slot_mut(surface) = None;
        }
    }

    /// The project a live surface is showing, if it is showing one.
    #[must_use]
    fn live_project(&self, surface: EmbedSurface) -> Option<&str> {
        self.live(surface)
            .map(|embedded| embedded.window.project.as_str())
    }

    /// True when the workspace slot already holds a live editor for this project. Opening it
    /// again is what the studio does every time it re-reads its state: a no-op, not an error.
    #[must_use]
    pub fn workspace_holds(&self, project: &str) -> bool {
        self.live_project(EmbedSurface::Workspace)
            .is_some_and(|held| crate::workspace::paths_match(held, project))
    }

    /// Which surfaces have to be closed before the workspace can open `project`.
    ///
    /// The host has one workspace slot and one game slot for the whole app, so any live
    /// surface on another project is in the way. The game comes first: it sits in front, and
    /// closing the editor underneath it would leave the viewport showing the old project.
    #[must_use]
    pub fn surfaces_to_close_for(&self, project: &str) -> Vec<EmbedSurface> {
        [EmbedSurface::Game, EmbedSurface::Workspace]
            .into_iter()
            .filter(|&surface| {
                self.live_project(surface)
                    .is_some_and(|held| !crate::workspace::paths_match(held, project))
            })
            .collect()
    }

    /// The ordered steps a launch of `surface` on `project` takes, given what the guards
    /// answered. Pure: it reads the host's two slots and nothing else, so the ordering that
    /// keeps the viewport honest is testable without a window, an engine or a project on disk.
    ///
    /// Two rules shape it.
    ///
    /// **The hole belongs to the project the user asked for.** Once the project is known to
    /// be one of the user's, every live surface on a *different* project is closed — before
    /// the refusal, not instead of it. A folder with no `project.godot`, a missing engine or
    /// an addon that will not install therefore leaves the viewport **empty**, never showing
    /// the project the user just navigated away from. The viewport and the project the studio
    /// says is active can only ever be the same thing.
    ///
    /// **The project that is already open is left alone.** Re-opening the workspace the
    /// viewport already holds is a no-op — the studio asks for it every time it re-reads its
    /// state — and a launch on that same project that the guards refuse closes nothing, so a
    /// failed Play never costs the user the editor they were working in.
    #[must_use]
    pub fn launch_plan(&self, project: &str, surface: EmbedSurface, verdict: Verdict) -> Vec<Step> {
        if verdict == Verdict::Ready
            && surface == EmbedSurface::Workspace
            && self.workspace_holds(project)
        {
            return Vec::new();
        }
        let mut steps: Vec<Step> = self
            .surfaces_to_close_for(project)
            .into_iter()
            .map(Step::Close)
            .collect();
        steps.push(match verdict {
            Verdict::Ready => Step::Launch,
            Verdict::Refused => Step::Refuse,
        });
        steps
    }

    /// The handle of an attached window, for a Win32 call.
    fn attached_hwnd(&self, surface: EmbedSurface) -> Option<isize> {
        self.live(surface)
            .filter(|embedded| embedded.window.attached)
            .map(|embedded| embedded.window.hwnd as isize)
    }

    /// Where every attached window should be right now: the front one over the viewport and
    /// shown (when the viewport is), the other hidden behind it.
    fn placements(&self, scale: f64) -> Vec<(isize, Option<PhysicalRect>)> {
        let front = self.front();
        let rect = self
            .viewport
            .filter(|viewport| !viewport.is_empty())
            .map(|viewport| viewport.to_physical(scale));
        [EmbedSurface::Workspace, EmbedSurface::Game]
            .into_iter()
            .filter_map(|surface| {
                let hwnd = self.attached_hwnd(surface)?;
                let shown = self.visible && front == Some(surface);
                Some((hwnd, if shown { rect } else { None }))
            })
            .collect()
    }
}

/// The managed handle.
pub type GodotEmbedHost = Arc<Mutex<EmbedHost>>;

fn lock(host: &GodotEmbedHost) -> Result<MutexGuard<'_, EmbedHost>, AppError> {
    host.lock().map_err(|_| AppError {
        message: "The viewport host is poisoned.".to_owned(),
        hint: Some("Restart the app; an earlier viewport call panicked.".to_owned()),
    })
}

fn emit_state(app: &tauri::AppHandle, host: &GodotEmbedHost) {
    if let Ok(host) = host.lock() {
        let _ignored = host.state().emit(app);
    }
}

// ── the parent window ────────────────────────────────────────────────────────────────

/// Bhippi's own window, as a Win32 parent: its handle and its scale factor.
#[derive(Clone, Copy, Debug)]
struct Parent {
    hwnd: isize,
    scale: f64,
}

fn parent_window(app: &tauri::AppHandle) -> Result<Parent, AppError> {
    let window = app
        .get_webview_window(MAIN_WINDOW)
        .ok_or_else(|| AppError {
            message: "The main window is not available.".to_owned(),
            hint: Some("Restart the app.".to_owned()),
        })?;
    let scale = window.scale_factor().map_err(|error| AppError {
        message: format!("Could not read the window's scale factor: {error}"),
        hint: Some("Restart the app.".to_owned()),
    })?;
    #[cfg(windows)]
    {
        let hwnd = window.hwnd().map_err(|error| AppError {
            message: format!("Could not read the window handle: {error}"),
            hint: Some("Restart the app.".to_owned()),
        })?;
        Ok(Parent {
            hwnd: hwnd.0 as isize,
            scale,
        })
    }
    #[cfg(not(windows))]
    {
        let _unused = scale;
        Err(unsupported())
    }
}

#[cfg(not(windows))]
fn unsupported() -> AppError {
    AppError {
        message: "The embedded Godot viewport is Windows-only in this build.".to_owned(),
        hint: Some(
            "Godot cannot be hosted inside Bhippi's window on this platform yet (ADR-0045)."
                .to_owned(),
        ),
    }
}

/// Position every attached window. Called with the host lock held; nothing here blocks.
fn apply_layout(host: &EmbedHost, parent: Parent) {
    for (hwnd, placement) in host.placements(parent.scale) {
        match placement {
            Some(rect) => win::place(hwnd, rect),
            None => win::set_visible(hwnd, false),
        }
    }
}

// ── commands ─────────────────────────────────────────────────────────────────────────

/// Open the project's workspace — the Godot editor — inside the viewport.
#[tauri::command]
#[specta::specta]
pub async fn godot_embed_open_workspace(
    app: tauri::AppHandle,
    state: tauri::State<'_, crate::Runtime>,
    store: tauri::State<'_, GodotSessionStore>,
    host: tauri::State<'_, GodotEmbedHost>,
    project: String,
) -> Result<GodotEmbedState, AppError> {
    launch(
        app,
        &state,
        store.inner().clone(),
        host.inner().clone(),
        project,
        EmbedSurface::Workspace,
    )
    .await
}

/// Run the game inside the viewport, on top of the workspace if one is open.
#[tauri::command]
#[specta::specta]
pub async fn godot_embed_play(
    app: tauri::AppHandle,
    state: tauri::State<'_, crate::Runtime>,
    store: tauri::State<'_, GodotSessionStore>,
    host: tauri::State<'_, GodotEmbedHost>,
    project: String,
) -> Result<GodotEmbedState, AppError> {
    launch(
        app,
        &state,
        store.inner().clone(),
        host.inner().clone(),
        project,
        EmbedSurface::Game,
    )
    .await
}

/// Close one surface. The game is asked to close and killed if it ignores the request; the
/// workspace is only asked, because the editor may be asking whether to save.
#[tauri::command]
#[specta::specta]
pub async fn godot_embed_stop(
    app: tauri::AppHandle,
    host: tauri::State<'_, GodotEmbedHost>,
    surface: EmbedSurface,
) -> Result<GodotEmbedState, AppError> {
    let host = host.inner().clone();
    let (hwnd, handle) = {
        let guard = lock(&host)?;
        match guard.live(surface) {
            Some(embedded) => (
                embedded
                    .window
                    .attached
                    .then_some(embedded.window.hwnd as isize),
                embedded.handle.clone(),
            ),
            None => return Ok(guard.state()),
        }
    };
    match hwnd {
        Some(hwnd) => {
            win::request_close(hwnd);
            if surface == EmbedSurface::Game {
                tauri::async_runtime::spawn(async move {
                    tokio::time::sleep(GAME_CLOSE_GRACE).await;
                    if !handle.is_stopped() {
                        tracing::info!("the game ignored its close request; killing it");
                        handle.kill();
                    }
                });
            }
        }
        // Not on screen yet: there is no window to ask, only a process to end.
        None => {
            handle.kill();
        }
    }
    emit_state(&app, &host);
    let guard = lock(&host)?;
    Ok(guard.state())
}

/// The page's side of the contract: the viewport's rect and whether it is unobstructed.
/// Called on every layout change; this is the hot path and it makes no bridge call.
#[tauri::command]
#[specta::specta]
pub async fn godot_embed_layout(
    app: tauri::AppHandle,
    host: tauri::State<'_, GodotEmbedHost>,
    rect: ViewportRect,
    visible: bool,
) -> Result<(), AppError> {
    let host = host.inner().clone();
    let changed = {
        let mut guard = lock(&host)?;
        let changed = guard.viewport != Some(rect) || guard.visible != visible;
        guard.viewport = Some(rect);
        guard.visible = visible;
        if guard.placements(1.0).is_empty() {
            return Ok(());
        }
        let parent = parent_window(&app)?;
        apply_layout(&guard, parent);
        changed
    };
    if changed {
        emit_state(&app, &host);
    }
    Ok(())
}

/// What the viewport currently holds.
#[tauri::command]
#[specta::specta]
pub async fn godot_embed_state(
    host: tauri::State<'_, GodotEmbedHost>,
) -> Result<GodotEmbedState, AppError> {
    let guard = lock(host.inner())?;
    Ok(guard.state())
}

// ── the launch ───────────────────────────────────────────────────────────────────────

async fn launch(
    app: tauri::AppHandle,
    state: &crate::Runtime,
    store: GodotSessionStore,
    host: GodotEmbedHost,
    project: String,
    surface: EmbedSurface,
) -> Result<GodotEmbedState, AppError> {
    // Refuse before anything is spawned: a window that cannot be embedded here must not open
    // somewhere else instead.
    let parent = parent_window(&app)?;
    // The only door: an unregistered path never reaches the viewport, and nothing below this
    // line touches a window until it has been through it. Once it has, the hole belongs to
    // this project whether or not the guards let it open.
    let root = resolve_project(state, &project).await?;
    let key = display_of(&root);
    // Without a project.godot Godot would open its Project Manager, and the viewport would
    // show a project picker instead of the project. Asked before the no-op below, as it
    // always has been, so a project that has lost its file is reported rather than reused.
    let is_godot_project = root.join("project.godot").is_file();
    // Opening the workspace that is already open — which is what the studio asks for every
    // time it re-reads its state — is a no-op, not a second editor and not an error. Asked of
    // the plan, so the command and the model of the command cannot drift apart.
    if is_godot_project {
        let guard = lock(&host)?;
        if guard.launch_plan(&key, surface, Verdict::Ready).is_empty() {
            return Ok(guard.state());
        }
    }
    let kind = surface.run_kind();

    // The guards. Every one of them reads the *incoming* project or the machine, and the only
    // write any of them makes goes into the incoming project — none of them can touch the
    // window the viewport is holding, so their answer is safe to collect before the plan
    // below decides what happens to that window.
    let guarded = if is_godot_project {
        guard_launch(state, &store, &root, &key, surface).await
    } else {
        Err(AppError {
            message: format!("{key} is not a Godot project yet: there is no project.godot."),
            hint: Some(
                "Describe the game in the chat so Bhippi builds it, or re-add the folder to scaffold it."
                    .to_owned(),
            ),
        })
    };

    // The plan, and then the plan carried out. Ownership of the viewport follows the project
    // the user asked for: the surfaces standing on another project are closed whatever the
    // guards answered, so a project that cannot open leaves the hole empty rather than
    // leaving the previous project's editor in it, pretending to be this one.
    let plan = {
        let guard = lock(&host)?;
        guard.launch_plan(
            &key,
            surface,
            if guarded.is_ok() {
                Verdict::Ready
            } else {
                Verdict::Refused
            },
        )
    };
    let mut closed = false;
    for step in &plan {
        if let Step::Close(standing) = *step {
            close_for_switch(&app, &host, standing).await;
            closed = true;
        }
    }
    if closed {
        // `workspace: None, game: None` — the page's empty state has to be on screen before
        // the refusal below reaches it, not after.
        emit_state(&app, &host);
    }
    // `Step::Refuse` is this `?`; `Step::Launch` is everything under it.
    let install = guarded?;

    let (handle, signal) = stop_channel();
    claim_slot(&store, &key, kind, handle.clone())?;
    announce_process(&app, &key, kind, GodotRunState::Starting, None);
    let token = {
        let mut guard = lock(&host)?;
        guard.begin(surface, &key, handle.clone())
    };
    emit_state(&app, &host);

    let spec = match surface {
        EmbedSurface::Workspace => editor_command(install.gui(), &root),
        EmbedSurface::Game => {
            // The GUI binary, so no console window flashes behind the game. A windowed run
            // has no timeout: it ends when the player closes it or presses Stop.
            let mut spec = run_command(
                install.gui(),
                &root,
                &RunOptions {
                    headless: false,
                    ..RunOptions::default()
                },
            );
            spec.timeout_secs = 0;
            spec
        }
    };
    let sender = start_output_pump(app.clone(), store.clone(), key.clone());
    let (pid_tx, pid_rx) = tokio::sync::oneshot::channel::<u32>();

    // The adopter: once the pid is known, look for the engine window and pull it in.
    {
        let app = app.clone();
        let host = host.clone();
        let handle = handle.clone();
        let key = key.clone();
        tauri::async_runtime::spawn(async move {
            let Ok(pid) = pid_rx.await else {
                return;
            };
            if let Ok(mut guard) = host.lock() {
                guard.set_pid(surface, token, pid);
            }
            emit_state(&app, &host);
            let deadline = Instant::now() + WINDOW_WAIT;
            loop {
                if handle.is_stopped() {
                    return;
                }
                let found = tokio::task::spawn_blocking(move || win::find_engine_window(pid))
                    .await
                    .ok()
                    .flatten();
                if let Some(hwnd) = found {
                    let adopted = {
                        match host.lock() {
                            Ok(mut guard) if guard.holds(surface, token) => win::adopt(
                                hwnd,
                                parent.hwnd,
                                guard.viewport.unwrap_or_default().to_physical(parent.scale),
                            )
                            .map(|()| {
                                guard.set_attached(surface, token, hwnd);
                                apply_layout(&guard, parent);
                            }),
                            // A project switch took this slot while the window was still on
                            // its way up. A Godot window with nowhere to go is closed, never
                            // left standing.
                            Ok(_) => Err("the viewport moved on to another project".to_owned()),
                            Err(_) => Err("the viewport host is poisoned".to_owned()),
                        }
                    };
                    match adopted {
                        Ok(()) => {
                            tracing::info!(
                                project = %key,
                                surface = surface.label(),
                                pid,
                                hwnd,
                                "Godot window embedded in the viewport"
                            );
                            emit_state(&app, &host);
                            keep_on_top(app.clone(), host.clone(), surface, handle.clone());
                        }
                        Err(error) => {
                            // A window Bhippi cannot embed must not stay open on its own.
                            tracing::warn!(%error, "could not embed the Godot window; stopping it");
                            handle.kill();
                        }
                    }
                    return;
                }
                if Instant::now() >= deadline {
                    tracing::warn!(
                        project = %key,
                        surface = surface.label(),
                        "the Godot window never appeared; stopping the process"
                    );
                    handle.kill();
                    return;
                }
                tokio::time::sleep(WINDOW_POLL).await;
            }
        });
    }

    // The run: streams output, ends the slot and the surface when the process exits.
    {
        let app = app.clone();
        let host = host.clone();
        let key = key.clone();
        tauri::async_runtime::spawn(async move {
            announce_process(&app, &key, kind, GodotRunState::Running, None);
            let result = run_spec_observed(
                &spec,
                Some(signal),
                move |pid| {
                    let _ignored = pid_tx.send(pid);
                },
                |line| {
                    let _ignored = sender.send(line);
                },
            )
            .await;
            release_slot(&store, &key, kind);
            if let Ok(mut guard) = host.lock() {
                guard.end_launch(surface, token);
                // The game ending reveals the workspace again.
                if let Ok(parent) = parent_window(&app) {
                    apply_layout(&guard, parent);
                }
            }
            emit_state(&app, &host);
            let exit = match result {
                Ok(exit) => Some(exit),
                Err(error) => {
                    tracing::warn!(message = %error.message, "the embedded Godot run failed to start");
                    None
                }
            };
            announce_process(&app, &key, kind, GodotRunState::Exited, exit);
        });
    }

    let guard = lock(&host)?;
    Ok(guard.state())
}

/// The guards that can still refuse a launch once the folder is known to be a registered
/// Godot project: the engine has to exist, and the workspace's editor addon has to be there.
///
/// Nothing here closes a window. Whatever it answers, the caller's plan clears the viewport
/// of the outgoing project first, so a refusal is returned to an empty hole.
async fn guard_launch(
    state: &crate::Runtime,
    store: &GodotSessionStore,
    root: &Path,
    key: &str,
    surface: EmbedSurface,
) -> Result<GodotInstall, AppError> {
    let install = require_install(state, store, key).await?;

    // The workspace is the Godot editor, and stock Godot opens it with the Scene, Import,
    // Inspector and FileSystem docks around the viewport — in a window that *is* Bhippi's
    // viewport, that is the whole hole spent on panels the studio already has in the page.
    // Bhippi's own editor addon turns Godot's distraction-free mode on once at startup; the
    // user gets the docks back with Ctrl+Shift+F12 whenever they want them. Installed here
    // rather than only at scaffold time so a project made by an older build gets it too, and
    // after `require_install` so a launch that was going to fail for a missing engine does
    // not write into the user's project first.
    //
    // A gate, not a warning: if the addon cannot be written the editor would come up with
    // the docks over the viewport, which is the bug this is here to prevent, so the launch
    // stops instead. The game path never runs this — a running game has no docks.
    if surface == EmbedSurface::Workspace {
        let addon_root = root.to_path_buf();
        let installed = tokio::task::spawn_blocking(move || {
            bhippi_engine::godot::scaffold::ensure_studio_addon(&addon_root)
        })
        .await
        .map_err(|error| AppError {
            message: format!("preparing {key} for the viewport did not finish: {error}"),
            hint: Some("Open the workspace again.".to_owned()),
        })?
        .map_err(|error| {
            let hint = error.hint().map(str::to_owned).unwrap_or_else(|| {
                "Check the project folder is writable and is not open in another Godot.".to_owned()
            });
            AppError {
                message: format!("{key} could not be prepared for the viewport: {error}"),
                hint: Some(hint),
            }
        })?;
        if installed {
            tracing::info!(
                project = %key,
                "installed the Bhippi studio editor addon so the workspace opens without its docks"
            );
        }
    }
    Ok(install)
}

/// Close a surface that is standing in the way of a project switch, and wait for it to go.
///
/// The window is asked the way its ✕ would ask, so the editor still gets to raise its
/// unsaved-scene prompt. If it is still there when [`SWITCH_CLOSE_GRACE`] runs out it is
/// killed: the alternative is a second engine left running on the old project with no slot
/// to be laid out in, which is the stray window INV-090 forbids.
async fn close_for_switch(app: &tauri::AppHandle, host: &GodotEmbedHost, surface: EmbedSurface) {
    let Some((hwnd, handle)) = ({
        let Ok(guard) = host.lock() else {
            return;
        };
        guard.live(surface).map(|embedded| {
            (
                embedded
                    .window
                    .attached
                    .then_some(embedded.window.hwnd as isize),
                embedded.handle.clone(),
            )
        })
    }) else {
        return;
    };
    match hwnd {
        Some(hwnd) => win::request_close(hwnd),
        // Still starting: there is no window to ask, only a process to end.
        None => {
            handle.kill();
        }
    }
    let deadline = Instant::now() + SWITCH_CLOSE_GRACE;
    loop {
        // The run task clears the slot when the process exits, and a killed handle stops
        // counting as live the moment it is asked to stop. Either way the slot is free.
        let gone = host
            .lock()
            .map_or(true, |guard| guard.live(surface).is_none());
        if gone {
            break;
        }
        if Instant::now() >= deadline {
            tracing::info!(
                surface = surface.label(),
                "the outgoing Godot window did not close in time; killing it for the switch"
            );
            handle.kill();
            break;
        }
        tokio::time::sleep(SWITCH_POLL).await;
    }
    emit_state(app, host);
}

/// Re-asserts the child's place above the webview between layout calls. The webview is a
/// sibling child window and can re-raise itself on its own resizes; this is the backstop
/// that makes sure the viewport never ends up painted over.
fn keep_on_top(
    app: tauri::AppHandle,
    host: GodotEmbedHost,
    surface: EmbedSurface,
    handle: GodotProcessHandle,
) {
    tauri::async_runtime::spawn(async move {
        let mut ticker = tokio::time::interval(KEEPER_TICK);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            ticker.tick().await;
            if handle.is_stopped() {
                return;
            }
            let Ok(guard) = host.lock() else {
                return;
            };
            if guard.attached_hwnd(surface).is_none() {
                return;
            }
            if let Ok(parent) = parent_window(&app) {
                apply_layout(&guard, parent);
            }
        }
    });
}

/// Adopt a window some other runner already found — the Computer Use playtest launches its
/// own game and hands the window here once it exists, so Watch play happens in the viewport
/// too. The process is owned by that runner; this only takes the window.
pub fn adopt_foreign_window(
    app: &tauri::AppHandle,
    host: &GodotEmbedHost,
    project: &str,
    process_id: u32,
    hwnd: u64,
    handle: GodotProcessHandle,
) -> Result<(), AppError> {
    let parent = parent_window(app)?;
    {
        let mut guard = lock(host)?;
        let token = guard.begin(EmbedSurface::Game, project, handle);
        guard.set_pid(EmbedSurface::Game, token, process_id);
        let rect = guard.viewport.unwrap_or_default().to_physical(parent.scale);
        win::adopt(hwnd as isize, parent.hwnd, rect).map_err(|error| AppError {
            message: format!("Could not embed the game window: {error}"),
            hint: Some("Stop the playtest and try again.".to_owned()),
        })?;
        guard.set_attached(EmbedSurface::Game, token, hwnd as isize);
        apply_layout(&guard, parent);
    }
    emit_state(app, host);
    Ok(())
}

/// The other half of [`adopt_foreign_window`]: the runner's process ended.
pub fn release_foreign_window(app: &tauri::AppHandle, host: &GodotEmbedHost) {
    if let Ok(mut guard) = host.lock() {
        guard.end(EmbedSurface::Game);
        if let Ok(parent) = parent_window(app) {
            apply_layout(&guard, parent);
        }
    }
    emit_state(app, host);
}

/// True when the viewport already holds a live game — the playtest command asks before it
/// launches, so two games never fight over the one hole.
pub fn game_in_viewport(host: &GodotEmbedHost) -> bool {
    host.lock()
        .map(|guard| guard.live(EmbedSurface::Game).is_some())
        .unwrap_or(false)
}

/// Stop everything the viewport holds. Called on the way out of the app.
pub fn shutdown(host: &GodotEmbedHost) {
    if let Ok(guard) = host.lock() {
        for surface in [EmbedSurface::Game, EmbedSurface::Workspace] {
            if let Some(embedded) = guard.live(surface) {
                embedded.handle.kill();
            }
        }
    }
    // The session store kills the same handles; this exists so the viewport is never the
    // one place that forgot.
    let _unused = lock_sessions;
}

// ── Win32 ────────────────────────────────────────────────────────────────────────────

// The one `unsafe` module in the product (ADR-0045): handle in, status out, a SAFETY note
// on every block. Nothing here allocates, retains a pointer, or outlives its call.
#[cfg(windows)]
#[allow(unsafe_code)]
mod win {
    use super::PhysicalRect;
    use windows_sys::Win32::Foundation::{HWND, LPARAM, RECT};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetClassNameW, GetWindowLongPtrW, GetWindowRect, GetWindowThreadProcessId,
        IsWindow, IsWindowVisible, PostMessageW, SetParent, SetWindowLongPtrW, SetWindowPos,
        ShowWindow, GWL_EXSTYLE, GWL_STYLE, HWND_TOP, SWP_FRAMECHANGED, SWP_NOACTIVATE,
        SWP_SHOWWINDOW, SW_HIDE, SW_SHOWNA, WM_CLOSE, WS_CAPTION, WS_CHILD, WS_CLIPSIBLINGS,
        WS_EX_APPWINDOW, WS_EX_CLIENTEDGE, WS_EX_DLGMODALFRAME, WS_EX_STATICEDGE, WS_EX_WINDOWEDGE,
        WS_MAXIMIZEBOX, WS_MINIMIZEBOX, WS_POPUP, WS_SYSMENU, WS_THICKFRAME, WS_VISIBLE,
    };

    /// Godot 4 names its window class `Engine`, for the game and the editor alike.
    const ENGINE_CLASS: &str = "Engine";

    struct Search {
        pid: u32,
        best: isize,
        best_area: i64,
    }

    unsafe extern "system" fn on_window(hwnd: HWND, lparam: LPARAM) -> i32 {
        // SAFETY: `lparam` is the `Search` this enumeration was started with, and it outlives
        // the `EnumWindows` call that runs this callback.
        let search = unsafe { &mut *(lparam as *mut Search) };
        let mut pid = 0u32;
        // SAFETY: `hwnd` is a live window handle supplied by `EnumWindows`.
        unsafe { GetWindowThreadProcessId(hwnd, &mut pid) };
        if pid != search.pid {
            return 1;
        }
        // SAFETY: as above.
        if unsafe { IsWindowVisible(hwnd) } == 0 {
            return 1;
        }
        let mut class = [0u16; 64];
        // SAFETY: the buffer is the length passed.
        let written = unsafe { GetClassNameW(hwnd, class.as_mut_ptr(), class.len() as i32) };
        let name = String::from_utf16_lossy(&class[..written.max(0) as usize]);
        if name != ENGINE_CLASS {
            return 1;
        }
        let mut rect = RECT {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        };
        // SAFETY: `rect` is a valid out pointer.
        unsafe { GetWindowRect(hwnd, &mut rect) };
        let area =
            i64::from(rect.right - rect.left).max(0) * i64::from(rect.bottom - rect.top).max(0);
        // The largest window wins: a splash or a tooltip is never the biggest one.
        if area >= search.best_area {
            search.best_area = area;
            search.best = hwnd as isize;
        }
        1
    }

    /// The engine's visible top-level window for a process, if it has one yet.
    pub fn find_engine_window(pid: u32) -> Option<isize> {
        let mut search = Search {
            pid,
            best: 0,
            best_area: -1,
        };
        // SAFETY: the callback only reads `search` for the duration of the call.
        unsafe {
            EnumWindows(Some(on_window), &mut search as *mut Search as LPARAM);
        }
        (search.best != 0).then_some(search.best)
    }

    /// Make `child` a frameless child of `parent`, placed over `rect`.
    pub fn adopt(child: isize, parent: isize, rect: PhysicalRect) -> Result<(), String> {
        let child_hwnd = child as HWND;
        let parent_hwnd = parent as HWND;
        // SAFETY: plain Win32 calls on handles this module was given; a stale handle makes
        // the calls fail, which is reported, not undefined behaviour.
        unsafe {
            if IsWindow(child_hwnd) == 0 {
                return Err("the Godot window is gone".to_owned());
            }
            // Hide first (SPA-405): between Godot creating its window and Bhippi re-parenting
            // it, the editor's boot splash would otherwise paint as a stray top-level window
            // on the desktop. It reappears only once it is a child, placed in the viewport.
            ShowWindow(child_hwnd, SW_HIDE);
            let frame = WS_POPUP
                | WS_CAPTION
                | WS_THICKFRAME
                | WS_MINIMIZEBOX
                | WS_MAXIMIZEBOX
                | WS_SYSMENU;
            let style = GetWindowLongPtrW(child_hwnd, GWL_STYLE) & !(frame as isize)
                | (WS_CHILD | WS_VISIBLE | WS_CLIPSIBLINGS) as isize;
            SetWindowLongPtrW(child_hwnd, GWL_STYLE, style);
            let ex_frame = WS_EX_APPWINDOW
                | WS_EX_WINDOWEDGE
                | WS_EX_CLIENTEDGE
                | WS_EX_DLGMODALFRAME
                | WS_EX_STATICEDGE;
            let ex_style = GetWindowLongPtrW(child_hwnd, GWL_EXSTYLE) & !(ex_frame as isize);
            SetWindowLongPtrW(child_hwnd, GWL_EXSTYLE, ex_style);
            if SetParent(child_hwnd, parent_hwnd).is_null() {
                return Err("SetParent refused the window".to_owned());
            }
            SetWindowPos(
                child_hwnd,
                HWND_TOP,
                rect.x,
                rect.y,
                rect.width,
                rect.height,
                SWP_FRAMECHANGED | SWP_SHOWWINDOW | SWP_NOACTIVATE,
            );
        }
        Ok(())
    }

    /// Move the child over `rect`, shown, above its siblings. Never steals focus.
    pub fn place(child: isize, rect: PhysicalRect) {
        // SAFETY: plain Win32 calls on a handle this module adopted.
        unsafe {
            SetWindowPos(
                child as HWND,
                HWND_TOP,
                rect.x,
                rect.y,
                rect.width,
                rect.height,
                SWP_SHOWWINDOW | SWP_NOACTIVATE,
            );
        }
    }

    pub fn set_visible(child: isize, visible: bool) {
        // SAFETY: as above.
        unsafe {
            ShowWindow(child as HWND, if visible { SW_SHOWNA } else { SW_HIDE });
        }
    }

    /// Ask the window to close the way its ✕ would. Asynchronous: Godot may answer with a
    /// save dialog, and the process ends only when it agrees.
    pub fn request_close(child: isize) {
        // SAFETY: as above.
        unsafe {
            PostMessageW(child as HWND, WM_CLOSE, 0, 0);
        }
    }
}

#[cfg(not(windows))]
mod win {
    use super::PhysicalRect;

    pub fn find_engine_window(_pid: u32) -> Option<isize> {
        None
    }

    pub fn adopt(_child: isize, _parent: isize, _rect: PhysicalRect) -> Result<(), String> {
        Err("embedding is Windows-only".to_owned())
    }

    pub fn place(_child: isize, _rect: PhysicalRect) {}

    pub fn set_visible(_child: isize, _visible: bool) {}

    pub fn request_close(_child: isize) {}
}

// ── tests ────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x: f64, y: f64, width: f64, height: f64) -> ViewportRect {
        ViewportRect {
            x,
            y,
            width,
            height,
        }
    }

    #[test]
    fn css_pixels_scale_to_physical_pixels_edge_by_edge() {
        let physical = rect(240.0, 40.0, 1000.5, 700.25).to_physical(1.5);
        assert_eq!(physical.x, 360);
        assert_eq!(physical.y, 60);
        // (240 + 1000.5) * 1.5 = 1860.75 → 1861, minus the left edge.
        assert_eq!(physical.width, 1861 - 360);
        assert_eq!(physical.height, (740.25f64 * 1.5).round() as i32 - 60);
    }

    #[test]
    fn a_bad_scale_is_treated_as_one_to_one() {
        assert_eq!(rect(10.0, 20.0, 30.0, 40.0).to_physical(0.0).width, 30);
        assert_eq!(rect(10.0, 20.0, 30.0, 40.0).to_physical(f64::NAN).x, 10);
    }

    #[test]
    fn a_collapsed_rect_never_becomes_a_zero_sized_child() {
        let physical = rect(5.0, 5.0, 0.0, 0.0).to_physical(2.0);
        assert_eq!((physical.width, physical.height), (1, 1));
        assert!(rect(5.0, 5.0, 0.0, 10.0).is_empty());
        assert!(!rect(5.0, 5.0, 1.0, 10.0).is_empty());
    }

    #[test]
    fn the_game_is_in_front_of_the_workspace_and_the_workspace_returns_when_it_ends() {
        let mut host = EmbedHost::default();
        assert_eq!(host.front(), None);
        // The signals stay alive: a kill on a channel nobody receives is not a stop, which is
        // exactly how a runner that already ended reads to the host.
        let (workspace, _workspace_signal) = stop_channel();
        host.begin(EmbedSurface::Workspace, "C:/games/demo", workspace);
        assert_eq!(host.front(), Some(EmbedSurface::Workspace));
        let (game, _game_signal) = stop_channel();
        host.begin(EmbedSurface::Game, "C:/games/demo", game.clone());
        assert_eq!(host.front(), Some(EmbedSurface::Game));
        // A stopped process is not a window, even before its slot is cleared.
        game.kill();
        assert_eq!(host.front(), Some(EmbedSurface::Workspace));
        host.end(EmbedSurface::Game);
        host.end(EmbedSurface::Workspace);
        assert_eq!(host.front(), None);
    }

    #[test]
    fn placements_show_only_the_front_window_and_only_while_visible() {
        let mut host = EmbedHost {
            viewport: Some(rect(100.0, 50.0, 800.0, 600.0)),
            visible: true,
            ..EmbedHost::default()
        };
        let (workspace, _workspace_signal) = stop_channel();
        let workspace_token = host.begin(EmbedSurface::Workspace, "p", workspace);
        host.set_attached(EmbedSurface::Workspace, workspace_token, 0x10);
        let (game, _game_signal) = stop_channel();
        let game_token = host.begin(EmbedSurface::Game, "p", game);
        // Not attached yet: nothing to place for the game, the workspace stays shown until
        // the game window actually exists.
        assert_eq!(host.placements(1.0).len(), 1);
        host.set_attached(EmbedSurface::Game, game_token, 0x20);
        let placements = host.placements(2.0);
        assert_eq!(
            placements,
            vec![
                (0x10, None),
                (
                    0x20,
                    Some(PhysicalRect {
                        x: 200,
                        y: 100,
                        width: 1600,
                        height: 1200
                    })
                )
            ]
        );
        host.visible = false;
        assert!(host
            .placements(2.0)
            .iter()
            .all(|(_, placement)| placement.is_none()));
    }

    #[test]
    fn reopening_the_workspace_that_is_already_open_is_a_no_op() {
        let mut host = EmbedHost::default();
        assert!(!host.workspace_holds("C:/games/demo"));
        let (workspace, _workspace_signal) = stop_channel();
        host.begin(EmbedSurface::Workspace, "C:/games/demo", workspace.clone());
        assert!(host.workspace_holds("C:/games/demo"));
        // The page sends the display path it was given; case and slashes must not matter.
        assert!(host.workspace_holds("c:\\games\\demo"));
        assert!(host.workspace_holds("C:/games/demo/"));
        assert!(!host.workspace_holds("C:/games/other"));
        // Nothing to reuse once the editor has stopped: that is a fresh launch.
        workspace.kill();
        assert!(!host.workspace_holds("C:/games/demo"));
    }

    #[test]
    fn switching_projects_closes_the_game_first_and_then_the_other_workspace() {
        let mut host = EmbedHost::default();
        assert!(host.surfaces_to_close_for("C:/games/next").is_empty());
        let (workspace, _workspace_signal) = stop_channel();
        host.begin(EmbedSurface::Workspace, "C:/games/demo", workspace);
        assert_eq!(
            host.surfaces_to_close_for("C:/games/next"),
            vec![EmbedSurface::Workspace]
        );
        let (game, _game_signal) = stop_channel();
        host.begin(EmbedSurface::Game, "C:/games/demo", game.clone());
        // The game sits in front, so it goes first.
        assert_eq!(
            host.surfaces_to_close_for("C:/games/next"),
            vec![EmbedSurface::Game, EmbedSurface::Workspace]
        );
        // The same project needs no switch at all, whatever it holds.
        assert!(host.surfaces_to_close_for("C:/GAMES/demo").is_empty());
        // A game that has already ended is bookkeeping, not a window in the way.
        game.kill();
        assert_eq!(
            host.surfaces_to_close_for("C:/games/next"),
            vec![EmbedSurface::Workspace]
        );
    }

    /// The bug this ordering exists for: with Demo Game's editor in the viewport, picking a
    /// folder that is not a Godot project used to leave Demo Game on screen, because the
    /// guard refused the new project before the switch ran. The viewport then showed a
    /// project the studio no longer said was active. Whatever the guards answer, the outgoing
    /// project goes first.
    #[test]
    fn a_refused_project_still_takes_the_viewport_from_the_one_it_replaces() {
        let mut host = EmbedHost::default();
        let (workspace, _workspace_signal) = stop_channel();
        host.begin(EmbedSurface::Workspace, "C:/games/demo", workspace);
        let (game, _game_signal) = stop_channel();
        host.begin(EmbedSurface::Game, "C:/games/demo", game);

        // No project.godot, no engine, no addon — one verdict, and it closes before it stops.
        assert_eq!(
            host.launch_plan("C:/games/demo2", EmbedSurface::Workspace, Verdict::Refused),
            vec![
                Step::Close(EmbedSurface::Game),
                Step::Close(EmbedSurface::Workspace),
                Step::Refuse,
            ]
        );
        // A project that opens closes exactly the same surfaces, in the same order.
        assert_eq!(
            host.launch_plan("C:/games/demo2", EmbedSurface::Workspace, Verdict::Ready),
            vec![
                Step::Close(EmbedSurface::Game),
                Step::Close(EmbedSurface::Workspace),
                Step::Launch,
            ]
        );
        // Play answers to the same rule: the hole belongs to the project that was asked for.
        assert_eq!(
            host.launch_plan("C:/games/demo2", EmbedSurface::Game, Verdict::Ready),
            vec![
                Step::Close(EmbedSurface::Game),
                Step::Close(EmbedSurface::Workspace),
                Step::Launch,
            ]
        );
    }

    #[test]
    fn the_project_that_is_already_open_is_never_closed_by_its_own_launch() {
        let mut host = EmbedHost::default();
        // Nothing live: a refusal is a refusal, with nothing to take down first.
        assert_eq!(
            host.launch_plan("C:/games/demo2", EmbedSurface::Workspace, Verdict::Refused),
            vec![Step::Refuse]
        );

        let (workspace, _workspace_signal) = stop_channel();
        host.begin(EmbedSurface::Workspace, "C:/games/demo", workspace);
        // Re-asking for the workspace that is open is what the studio does on every state
        // read: nothing to close, nothing to spawn, nothing to say.
        assert!(host
            .launch_plan("C:/games/demo", EmbedSurface::Workspace, Verdict::Ready)
            .is_empty());
        assert!(host
            .launch_plan("c:\\games\\demo\\", EmbedSurface::Workspace, Verdict::Ready)
            .is_empty());
        // Play on that same project with no engine installed refuses — and leaves the editor
        // the user is working in exactly where it was.
        assert_eq!(
            host.launch_plan("C:/games/demo", EmbedSurface::Game, Verdict::Refused),
            vec![Step::Refuse]
        );
        assert_eq!(
            host.launch_plan("C:/games/demo", EmbedSurface::Workspace, Verdict::Refused),
            vec![Step::Refuse]
        );
    }

    #[test]
    fn the_state_after_a_refused_switch_is_the_pages_empty_state() {
        let mut host = EmbedHost::default();
        let (workspace, _workspace_signal) = stop_channel();
        host.begin(EmbedSurface::Workspace, "C:/games/demo", workspace.clone());
        assert_eq!(
            host.launch_plan("C:/games/demo2", EmbedSurface::Workspace, Verdict::Refused),
            vec![Step::Close(EmbedSurface::Workspace), Step::Refuse]
        );

        // What `close_for_switch` leaves behind: the handle stopped, then the slot cleared by
        // the outgoing run task. Either half on its own is already enough for the page.
        workspace.kill();
        let state = host.state();
        assert!(state.workspace.is_none());
        assert!(state.game.is_none());
        assert!(state.front.is_none());
        host.end(EmbedSurface::Workspace);
        assert_eq!(host.state(), GodotEmbedState::default());
    }

    #[test]
    fn a_late_run_task_cannot_clear_the_slot_the_next_project_already_took() {
        let mut host = EmbedHost::default();
        let (first, _first_signal) = stop_channel();
        let first_token = host.begin(EmbedSurface::Workspace, "C:/games/demo", first);
        let (second, _second_signal) = stop_channel();
        let second_token = host.begin(EmbedSurface::Workspace, "C:/games/next", second);
        assert_ne!(first_token, second_token);
        assert!(!host.holds(EmbedSurface::Workspace, first_token));
        assert!(host.holds(EmbedSurface::Workspace, second_token));

        // The outgoing engine's adopter and run task both finish late. Neither may touch the
        // window that took their slot.
        host.set_pid(EmbedSurface::Workspace, first_token, 111);
        host.set_attached(EmbedSurface::Workspace, first_token, 0x30);
        host.end_launch(EmbedSurface::Workspace, first_token);
        let state = host.state();
        let live = state.workspace.expect("the new workspace is still there");
        assert_eq!(live.project, "C:/games/next");
        assert_eq!(live.process_id, 0);
        assert!(!live.attached);

        // Its own launch still owns it.
        host.set_pid(EmbedSurface::Workspace, second_token, 222);
        assert_eq!(
            host.state().workspace.map(|window| window.process_id),
            Some(222)
        );
        host.end_launch(EmbedSurface::Workspace, second_token);
        assert!(host.state().workspace.is_none());
    }

    #[test]
    fn the_state_the_page_sees_uses_snake_case_surfaces() {
        let mut host = EmbedHost::default();
        let (game, _game_signal) = stop_channel();
        let token = host.begin(EmbedSurface::Game, "p", game);
        host.set_pid(EmbedSurface::Game, token, 4242);
        let json = serde_json::to_value(host.state()).expect("serialises");
        assert_eq!(json["front"], "game");
        assert_eq!(json["game"]["process_id"], 4242);
        assert_eq!(json["game"]["attached"], false);
        assert!(json["workspace"].is_null());
    }
}
