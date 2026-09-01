# ADR-0019 — Desktop-wide Computer Use overlay and custom pointer

- **Status:** Accepted
- **Date:** 2026-08-27
- **Supersedes:** nothing
- **Relates to:** ADR-0015 (computer use & vision automation), ADR-0018 (handoff & engagement), `docs/12-COMPUTER-USE-IMPLEMENTATION-PLAN.md` Phase 4

## Context

Phase 4 of the Computer Use plan requires a transparent, always-on-top, click-through
overlay spanning the whole virtual desktop, drawing Bhippi's own pointer so the user can
watch the agent work. The current aura (`ComputerUseAura`) animates only inside the Bhippi
window; when the agent drives the desktop, the user's view and the agent's focus leave the
window and nothing follows them.

The owner also asked for two specifics on top of the plan:

1. The overlay should animate like the reactbits "grid-scan" background — a 3D perspective
   grid receding into depth with a scan band sweeping through it — so an active turn *reads*
   as a deep scan of the desktop, not a cosmetic ring.
2. The Windows cursor should be replaced while a turn runs with a black arrow wearing a
   yellow aura, instead of the stock arrow stamped over our graphics.

## Decision

### 1. A second, hidden, desktop-scoped webview window

`lib.rs` setup creates a `WebviewWindow` named `overlay`:

- `visible(false)`, `decorations(false)`, `transparent(true)`, `always_on_top(true)`,
  `skip_taskbar(true)`, `shadow(false)`, `resizable(false)`, `maximizable(false)`,
  `minimizable(false)`, `focusable(false)`, background colour fully transparent.
- At activation it is positioned and sized (`Physical`) to the current virtual desktop
  bounds from `computer::screen_bounds()` (the same shim the screenshots use, so the
  coordinate contract of ADR-0018 holds 1:1), and `set_ignore_cursor_events(true)` makes it
  fully click-through.
- It serves `overlay.html` — a second Vite entry (`ui/src/overlay.tsx`) that renders the
  grid-scan aura and nothing else (no app chrome).

### 2. Engine-driven lifecycle, atomic by construction

`ChatEngine` gains a `desktop_overlay: Option<tauri::AppHandle>` field set through a
`.with_overlay(handle)` builder in `run()`. At the top of `run_computer_turn`, after the
state mark, a RAII `OverlayGuard` is created; its `Drop` hides the overlay. Because the guard
lives for the whole function body, **every** exit path — Done, Failed, fault, cancellation —
closes the overlay without threading an explicit call through each `return`.

The guard is a no-op in unit tests and the headless CLI (the field defaults to `None`).

### 3. Shown/hidden through a small `overlay` module

`crates/bhippi-app/src/overlay.rs` owns the window and a `STATE` mutex:

- Activation emits `computer-overlay-show` (label + virtual origin/size), blanks the Windows
  cursor, shows the window, and starts a persistent PowerShell loop that streams
  `GetCursorPos` pairs as `computer-overlay-cursor` events (throttled to ~12 ms).
- The same loop watches global Escape key-down edges. Two distinct presses within 900 ms
  publish a generation-scoped emergency-stop signal directly to `run_computer_turn`; the
  active provider stream is cancelled, the OS cursor is restored, and a late signal from an
  older turn cannot stop a newer one. The HUD always prints “Press Esc twice to stop”.
- Deactivation emits `computer-overlay-hide`, restores the OS cursor via
  `SystemParametersInfo(SPI_SETCURSORS)`, stops the watcher, and hides the window after a
  420 ms fade. A monotonically increasing generation counter stops a late fade from hiding a
  newly re-shown window.
- `blank_system_cursor`/`restore_system_cursor` live in `computer.rs` with the other
  PowerShell bridges; the transparent arrow is a 32×32 `CreateCursor` with zeroed AND/XOR
  masks (`SetSystemCursor`), restore reloads the user's real scheme, and it is called again
  on `RunEvent::Exit` so a killed app never leaves an invisible cursor behind.
- Events are raw Tauri `emit` calls (serde_json payloads) to the `overlay` window only, not
  through specta: the overlay page is internal chrome that never needs generated bindings, so
  `ui/src/lib/ipc.ts` stays byte-identical (INV-032 keeps the typed surface as the *app's*
  contract).

### 4. The aura is a perspective grid + depth sweep

The existing `ComputerUseAura` canvas is upgraded from a breathing peripheral mesh to a
pseudo-3D render: a floor grid converging on a vanishing point, depth fog, and a scan band
that sweeps from near to far in a continuous loop while its leading edge blooms amber. The
HUD pill, perimeter streaks, and corner brackets are unchanged, and the whole overlay is
transparent so the desktop stays visible underneath.

### 5. Custom pointer

`ui/src/components/OverlayCursor.tsx` draws a black arrow with a yellow rim inside a soft
pulsing yellow aura at the last reported cursor position, smoothed with a lerp so the 80 Hz
event stream reads as one continuous pointer. The OS arrow is blanked for the turn, so the
custom arrow is what the user sees; if blanking ever fails the turn continues (logged, never
blocking).

## Consequences

- The overlay is precise on uniform-DPI displays (this machine runs 1920×1080 at 100 %). On
  mixed-DPI multi-monitor layouts the window sits at the primary monitor's scale for the
  whole webview; CSS coordinates are derived as `(virtual − origin) / devicePixelRatio`, so a
  second monitor at a different scale can draw the cursor a few pixels off — documented,
  acceptable for v1.
- A PowerShell process runs per active Computer Use turn (one persistent watcher). It dies
  when the turn ends; if the app is hard-killed the orphan writes to a broken pipe, fails
  under `-ErrorActionPreference Stop`, and exits on its own.
- Overlay event payloads use camelCase explicitly, matching the TypeScript listener. This is
  required for non-zero virtual-desktop origins and for the replacement pointer to follow the
  real mouse instead of receiving undefined origin coordinates.
- The in-app aura mount and its `onComputerActionChange` wiring are removed from
  `App.tsx`/`Chat.tsx`; every Computer Use turn is now surfaced on the desktop instead.
  The Settings › Computer Use "test capture" button is not affected: it never enters
  `run_computer_turn`, so no overlay flashes for a manual preview.
- Live proof is a manual desktop run (vision providers are billing-limited), matching the
  existing `computer_loop_live.rs` pattern of `#[ignore]`-gated Windows-only checks.

## Alternatives

- **Frontend-driven switching** (the old aura heuristic calls a new IPC command). Rejected:
  the chat screen unmounts when the user navigates away, so a turn started on Chat would
  lose its overlay mid-flight, and the engine is the one place that knows a desktop turn
  really started.
- **`windows` crate / raw FFI for cursor + position.** Rejected: the workspace forbids
  `unsafe` and the PowerShell shim is the established pattern (ADR-0015/16/18).
- **Three.js/postprocessing for the grid-scan look.** Rejected: adds two dependencies for a
  2D-canvas approximation the plan's performance budget does not justify.
- **Hiding the cursor by moving it to the corner.** Rejected: the visible cursor is the
  point of the feature.
