# ADR-0028 — The webview is the shipping viewport renderer

- **Status:** Accepted
- **Date:** 2026-09-01
- **Amends:** ADR-0020 (§child-process viewport model), INV-072, INV-073, INV-077
- **Relates to:** ADR-0022 (engine pipeline), `docs/13-ENGINE-AI-CONTROL-AND-UNREAL-UX-PLAN.md` ENG-168

## Context

ADR-0020 specified the 3D viewport as a **Bevy child process** embedded into the workbench
(Windows `SetParent`), driven over a stdio JSON-RPC channel, with INV-072 guaranteeing that a
viewport crash lands in the pane's error state rather than the shell's.

That design has not been built. What exists today is:

- `crates/bhippi-engine-viewport/src/protocol.rs` — 208 lines of complete, well-formed
  JSON-RPC types (`editor.open_scene`, `editor.screenshot`, `PlayStats`, …), **never called
  by anything**.
- `crates/bhippi-engine-viewport/src/bevy.rs` — a **13-line stub** whose `run()` builds
  `App::new().add_plugins(DefaultPlugins)` and returns. There is no window, no grid, no
  camera, no picking.
- `ui/src/engine/EngineViewport.tsx` — a Three.js viewport that has carried the product
  since 2026-08-30: orbit and fly cameras, transform gizmos with snap and world/local space,
  click picking, an axis widget, weather, and (since ENG-146) an accumulated transform
  hierarchy.

ENG-010 ("Bevy child-process viewport at 55 fps") has been listed as *next* in three
consecutive session logs (2026-08-29, 2026-08-30, 2026-09-01) without being started. Phase 6
of the engine plan — physics, character controller, script runtime — cannot begin until it is
settled where the runtime lives, because that decides whether physics runs in Rust beside a
Bevy app or in the webview beside the Three.js scene.

Leaving it open has a cost that is now being paid every phase: two viewport stories, one of
which is real, and a performance invariant (INV-077) that names a renderer nobody is
building.

## Decision

**The webview viewport is the shipping renderer.** ADR-0020's child-process model is
withdrawn, not deferred.

Concretely:

1. `ui/src/engine/EngineViewport.tsx` is the viewport, and is held to a real performance
   budget rather than an aspirational one (INV-077 below).
2. `bhippi-engine-viewport` keeps `protocol.rs` — it is a good description of an editor
   control channel and costs nothing to keep — but the `bevy` feature and `bevy.rs` stub are
   **removed**, along with the Bevy dependency. A 13-line stub that cannot run is worse than
   an honest absence: it makes the crate look half-built rather than unbuilt.
3. Phase 6's physics and script runtime run **in the webview**, beside the scene they act on.
4. The engine crates stay exactly as they are. Nothing about this decision moves scene state,
   transactions, undo, validation, the schema registry, asset indexing or composition out of
   Rust; those are what INV-073 is actually protecting, and they remain protected.

### Why this way

- **It is what is true.** One renderer ships. Writing an ADR that keeps promising the other
  one is the "fake breadth" the engine plan's own status tracker warns against.
- **The cost of the Bevy path is not the renderer.** It is child-process lifecycle, Windows
  window embedding, resize and DPI synchronisation, input forwarding, a JSON-RPC transport on
  the hot path, and INV-072's crash-isolation contract — weeks of work whose deliverable is
  *the same picture in the same rectangle*.
- **The frame path gets shorter, not longer.** A child process means every selection, gizmo
  drag and camera move crosses a process boundary. In the webview the viewport reads the same
  `EngineSceneState` the panels already render.
- **The perf argument is not settled in Bevy's favour at this scale.** INV-077 asks for 55 fps
  at 1 000 entities. That is well inside what WebGL2 with instancing and frustum culling
  does, and ENG-167 measures it rather than assuming it.

### What is genuinely lost

Stated plainly, because a decision that only lists upsides is not a decision:

- **Ceiling.** A native renderer would eventually beat WebGL for very large worlds, compute
  shaders and advanced GI. If Bhippi ever needs those, this ADR is superseded — and by then
  `protocol.rs` is still there.
- **Crash isolation.** INV-072 guaranteed a viewport crash could not take the shell down.
  In-process, a renderer panic is a webview error. Mitigated by an error boundary around the
  viewport pane, not by process isolation.
- **INV-073's letter.** Rendering and raycast picking now provably live in TypeScript. This
  ADR amends the invariant to say so explicitly rather than leaving the codebase quietly in
  breach of it.

## Consequences

**Invariants amended:**

- **INV-072** (viewport is a child process; kill/restart lands in the pane's error state) —
  **retired**. Replaced by: *the viewport pane is wrapped in an error boundary; a renderer
  failure shows the pane's error state with a Reload action and never blanks the shell.*
- **INV-073** (the webview computes nothing for the engine) — **narrowed**, and made precise:
  *scene state, transactions, undo, validation, the schema and widget registries, asset
  indexing, HUD rect resolution and play composition live in `bhippi-engine`. Rendering,
  raycast picking against rendered meshes, and camera navigation are the webview's, because
  they are properties of the picture rather than of the document.* Everything previously
  listed stays where it is.
- **INV-077** (≥55 fps, 1 000 entities, editor mode) — **kept, re-targeted** at the webview
  viewport and made measurable by ENG-167's stats harness. An unmeasured budget is a wish.
- **INV-078** (cold attach ≤3 s: viewport spawn → first frame) — **retired**; there is no
  process to spawn. The pane's mount time is covered by the existing lazy-mount behaviour.

**Crates:** `bhippi-engine-viewport` loses its `bevy` feature, its `bevy.rs`, its Bevy
dependency and its binary target. `protocol.rs` remains as a documented, unused control-channel
design, marked as such.

**Build order:** ENG-010…013 (the P1 viewport spike) are closed as **withdrawn**, not done.
ENG-168 is closed by this ADR.

## Alternatives rejected

- **Build the Bevy viewport now.** Rejected on cost against benefit at this stage: weeks of
  embedding and IPC work to draw the same scene, while the renderer still cannot draw a
  material at all (F8). The right time for a native renderer is when the webview's ceiling is
  the thing actually blocking someone, and that is measurable — not now.
- **Keep both, decide later.** This is the status quo, and it is what produced a 13-line stub,
  an unused protocol, and an invariant naming a renderer nobody was building. "Later" has
  already cost three sprints.
- **Bevy headless for physics only, webview for rendering.** Splits the world across two
  representations that must agree every frame, which is strictly harder than either single
  choice.

## Reversal

If the webview ceiling becomes the blocker, this ADR is superseded by one that reinstates the
child-process model. `protocol.rs` is the starting point, INV-072 comes back with it, and the
scene/transaction/asset layers need no change — which is precisely why keeping them in Rust
mattered regardless of which renderer wins.
