# Plan for the 3D Workflow — the Bhippi Game Engine Workbench

**Doc:** planfor3dworkflow.md
**Derives from:** 00-SPEC-v1.0, 01-ARCHITECTURE, 04-PAGES, 06-INVARIANTS, ADR-0012..0019 (ADE shell, workbench, computer use)
**Status:** proposal — requires an ADR (`docs/adr/0020-game-engine-workbench.md`) before any code lands
**Authority:** below 00-SPEC, 06-INVARIANTS and the ADRs. Where this document conflicts with them, they win.

---

## Table of contents

1. [Executive summary](#1-executive-summary)
2. [Where this fits inside Bhippi today](#2-where-this-fits-inside-bhippi-today)
3. [Non-negotiable constraints inherited from the project](#3-non-negotiable-constraints-inherited-from-the-project)
4. [Engine foundation decision — build on Bevy](#4-engine-foundation-decision--build-on-bevy)
5. [High-level architecture](#5-high-level-architecture)
6. [Viewport rendering strategy](#6-viewport-rendering-strategy)
7. [Editor UI specification (the full Unreal/Unity-class surface)](#7-editor-ui-specification)
8. [Game project layout on disk](#8-game-project-layout-on-disk)
9. [Scene format, asset identity, and the data model](#9-scene-format-asset-identity-and-the-data-model)
10. [Component model and scripting](#10-component-model-and-scripting)
11. [Asset pipeline](#11-asset-pipeline)
12. [Editing semantics: transactions, undo/redo, autosave](#12-editing-semantics-transactions-undoredo-autosave)
13. [The Engine Mind Map — how the AI sees the engine](#13-the-engine-mind-map--how-the-ai-sees-the-engine)
14. [AI ↔ Engine interaction protocol](#14-ai--engine-interaction-protocol)
15. [Chat, CLI, and slash-command integration](#15-chat-cli-and-slash-command-integration)
16. [Play mode and simulation](#16-play-mode-and-simulation)
17. [Build & deployment system (Windows / Android / iOS / Web / more)](#17-build--deployment-system)
18. [Runtime subsystem stack (physics, audio, animation, UI, particles)](#18-runtime-subsystem-stack)
19. [Open-source repositories to adopt (do not build from scratch)](#19-open-source-repositories-to-adopt)
20. [IPC command surface and event catalogue](#20-ipc-command-surface-and-event-catalogue)
21. [Database additions](#21-database-additions)
22. [Performance budgets](#22-performance-budgets)
23. [Testing strategy](#23-testing-strategy)
24. [Phased build order with tickets](#24-phased-build-order-with-tickets)
25. [Risks and mitigations](#25-risks-and-mitigations)
26. [Glossary](#26-glossary)

---

## 1. Executive summary

Bhippi's workbench today has two modes — **Editor** and **Browser** — mounted as a split
panel next to the chat. This plan adds a third mode: **Engine**.

Engine mode is a full game-engine editor in the spirit of Unreal and Unity:

- a real-time **3D viewport** (WASD fly camera, orbit, gizmos, grid, picking, snapping),
- a **Scene Hierarchy** panel (the entity tree of the open scene),
- an **Inspector** panel (components of the selected entity, live-editable),
- a **Content Drawer / Asset Browser** mirroring an on-disk `assets/` folder structure,
- a **Console** (engine logs, script errors, build output),
- **Play / Pause / Stop** in-editor simulation,
- a **Build panel** that packages the game for **Windows, macOS, Linux, Android, iOS,
  and Web/HTML5** with progress streaming into the chat's ActivityDock.

Two audiences drive every design decision, with equal priority:

1. **The human.** Everything is manually editable: drag a model from the asset browser
   into the viewport, move it with gizmos, retune a light in the inspector, rebuild a
   level by hand — exactly the muscle memory of Unreal/Unity.
2. **The AI.** The chat agent (and any CLI provider Bhippi drives) can *see* and *edit*
   the same world through a structured, machine-readable protocol: a persistent
   **Engine Mind Map** (`.bhippi/engine-map.json`, a queryable index of every scene,
   entity, asset, script, and setting), a typed **engine action** channel (mirroring the
   existing `<write_file>` / `<computer_action>` mechanism), and viewport screenshots for
   visual grounding. The AI can move one prop, or redesign an entire level, or generate
   a new scene from a prompt — through the identical transaction system the human uses,
   so undo/redo, permissions, and audit apply uniformly.

We do **not** write a renderer, physics engine, or asset importer from scratch. The
engine core is **Bevy** (MIT/Apache-2.0, pure Rust, ECS, wgpu-based, first-class
Windows/macOS/Linux/Android/iOS/WASM targets) plus a curated set of Bevy-ecosystem
crates (Avian physics, transform-gizmo, bevy-inspector-egui internals as reference,
bevy_infinite_grid, kira audio, bevy_remote for the AI wire protocol). Section 19 lists
every repository, its license, and what we take from it.

Estimated shape: **3 new Rust crates** (`bhippi-engine`, `bhippi-engine-viewport`,
`bhippi-engine-build`), **1 new UI directory** (`ui/src/engine/`), **~40 new IPC
commands**, **~15 new coalesced events**, and **8 delivery phases** (Section 24).

---

## 2. Where this fits inside Bhippi today

### 2.1 The seam we plug into

The sanctioned extension point is the workbench (`ui/src/workbench/`, ADR-0014):

```
┌────────────────────────────────────────────────────────────────────────┐
│ TitleBar (40px)                                                        │
├──────────┬─────────────────────────────┬───────────────────────────────┤
│          │                             │  Workbench (toggle, 400–900px │
│ Sidebar  │   Chat / Research / ...     │  splitter, both panes stay    │
│ (280px)  │                             │  mounted)                     │
│          │                             │  ┌─────────────────────────┐  │
│          │                             │  │ ModeSwitch:             │  │
│          │                             │  │ [Editor|Browser|ENGINE] │  │◄── new third pill
│          │                             │  ├─────────────────────────┤  │
│          │                             │  │  EngineWorkbench        │  │
│          │  ActivityDock + composer    │  │  (this document)        │  │
├──────────┴─────────────────────────────┴──┴─────────────────────────┴──┤
│ StatusBar (28px)                                                       │
└────────────────────────────────────────────────────────────────────────┘
```

- `ModeSwitch.tsx` grows from two pills to three: **Editor · Browser · Engine**.
  Keyboard: `Ctrl/Cmd+B` opens the workbench (existing), `Ctrl/Cmd+'` cycles modes
  (existing behaviour extended to a 3-cycle), `Ctrl/Cmd+3` jumps straight to Engine.
- The Engine pane stays mounted once opened, like Editor and Browser, so switching
  modes never tears down the viewport or loses selection.
- Because a docked pane between 400–900px is cramped for level design, the Engine mode
  adds one thing the other modes don't have: a **Maximize** control (`F11` inside the
  pane) that expands the workbench over the chat column, leaving only the sidebar and
  status bar. `Esc` or `F11` restores the split. The chat stays reachable via a slim
  reopen handle — level design and conversation must be able to coexist, since the core
  loop is "tell the agent something, watch it act in the viewport."

### 2.2 What kind of projects get an Engine

A Bhippi project becomes a *game project* when it contains a `Bhippi.game.toml`
manifest at its root (created by the New Game Project flow or by hand). If the open
project has no manifest, the Engine pill still appears but shows the **empty state**:
"This project has no game manifest — Create one?" (one-click scaffold). This keeps the
project-first shell unchanged: one project = one workspace = one optional game.

### 2.3 Relationship to the product's research mission

The research pipeline (harvest → mind map → writer → publish) is untouched. The engine
is a *workbench capability* of the ADE shell, the same category of feature as the code
editor, browser, and computer use — tools the agent uses to build things for the user.
The ADR must state this explicitly so the "scope creep to a general research tool" risk
in 00-SPEC is addressed head-on: the domain lock applies to the *research/publishing*
pipeline, not to what the coding agent can build in a workspace.

---

## 3. Non-negotiable constraints inherited from the project

Every design in this document is shaped by these. Reviewers should check each PR
against this table.

| # | Constraint | Source | Consequence for the engine |
|---|---|---|---|
| C1 | The webview computes nothing; all logic in Rust | R3 / L-4 / INV-051 | Scene state, picking math, gizmo math, undo stacks, asset scanning — all in `bhippi-engine`. TypeScript renders panels and forwards input only. |
| C2 | IPC types are generated, never hand-edited | INV-032 | Every engine command goes through `commands.rs` + specta; regenerate `ui/src/lib/ipc.ts`. |
| C3 | Events are facts, never commands; coalesced ≤20/s | INV-021 | Transform-drag updates, play-mode stats, build progress all flow as coalesced events. The 3D viewport itself does NOT redraw over IPC (Section 6). |
| C4 | No SQL outside `bhippi-db` | INV-042 | New tables (Section 21) get repositories in `bhippi-db`. |
| C5 | No `unwrap()` outside tests; `unsafe` forbidden workspace-wide | INV-036 | Engine crates inherit workspace lints. Bevy itself is a dependency, not our code. |
| C6 | Crate dependency graph is enforced by `tests/architecture.rs` | INV-060 | New crates and edges must be added to the table in 01-ARCHITECTURE §3.1 via the ADR. |
| C7 | CPU-bound work on `spawn_blocking`; one Tokio runtime | INV-043 | Asset imports, builds, scene serialisation run on blocking tasks; the engine sim runs on its own dedicated thread (Section 5.4) which is standard for game loops. |
| C8 | Every screen implements loading / empty / error / populated + a11y floor | INV-034 | Every engine panel (hierarchy, inspector, assets, console, build) specifies all four states in Section 7. |
| C9 | Prompts are versioned files in `prompts/` | INV-035 | New `prompts/chat-engine.md` teaches the agent the engine protocol. |
| C10 | Permission gates block, never warn | gate philosophy | Destructive engine actions from the AI (delete entities, overwrite scene, run build) go through the existing `chat-permission-requested` flow. |
| C11 | No secrets outside the keychain | INV-00x | Android keystore passwords and iOS signing identities are referenced from the keychain, never stored in `Bhippi.game.toml`. |
| C12 | Path confinement | `files.rs` precedent | All engine file operations are confined to the project workspace, same guard as workbench file writes. |

---

## 4. Engine foundation decision — build on Bevy

### 4.1 Candidates considered

| Option | What it is | Verdict |
|---|---|---|
| **Bevy** (github.com/bevyengine/bevy) | Pure-Rust ECS engine on wgpu. MIT/Apache-2.0. Targets Windows, macOS, Linux, Android, iOS, WASM/WebGL2/WebGPU. Huge ecosystem, official **Bevy Remote Protocol** (JSON-RPC over HTTP for live ECS inspection/mutation), scene format, glTF pipeline, animation, UI. | **Chosen.** Fits the Rust-only-logic rule perfectly; the runtime, the editor sim, and Bhippi share one language, one workspace, one toolchain. BRP is a gift for AI control. |
| Godot embedded via godot-rust/gdext | Mature full editor, but a C++ engine with its own editor process, scene format, and GDScript culture. | Rejected as the core: embedding Godot's *editor* is impractical; driving Godot headless from Rust gives us the worst of both worlds. Kept as an *interop* idea only (glTF exchange). |
| Fyrox (github.com/FyroxEngine/Fyrox) | Rust engine that ships its own editor (Fyroxed). | Serious alternative; smaller ecosystem than Bevy, retained-mode scene graph rather than ECS, weaker mobile story. Study its editor code (MIT) for editor UX patterns, but don't adopt. |
| three.js / Babylon.js viewport with Rust "backend" | Web-native 3D in the webview. | Rejected: it drags scene logic into TypeScript or forces a chatty IPC render loop; violates C1 in spirit; and cannot be the *shipping runtime* for native/mobile builds, meaning editor and game would diverge. |
| Write our own on wgpu | Full control. | Rejected: multi-year effort; the user explicitly asked to reuse existing repos. |

### 4.2 Why Bevy specifically wins for Bhippi

1. **One language, one build graph.** `bhippi-engine` links Bevy as a normal Cargo
   dependency. The architecture test, lint wall, and MSRV policy apply uniformly.
2. **Editor sim == shipped runtime.** The game that runs in the viewport is the same
   Bevy `App` (minus editor plugins) that ships to Android/iOS/Web. No "editor
   approximation" drift, which is a chronic Unity/Unreal pain.
3. **Bevy Remote Protocol (BRP).** Bevy ships `bevy_remote`: a JSON-RPC 2.0 server
   (methods like `world.get_components`, `world.query`, `world.spawn_entity`,
   `world.insert_components`, `world.destroy_entity`, `registry.schema`) designed
   exactly for external tools to inspect and mutate a live world. Our AI protocol
   (Section 14) is a thin, permissioned wrapper over BRP semantics rather than an
   invented wire format.
4. **Reflection + scene serialisation built in.** `bevy_reflect` gives us typed,
   schema-discoverable components — the Inspector panel and the Engine Mind Map both
   fall out of the reflection registry instead of hand-maintained metadata.
5. **Ecosystem coverage** for everything an engine needs (Section 18/19): physics,
   audio, gizmos, grids, navmesh, particles, tweening, input mapping, hot-reload.
6. **Proven mobile/web pipelines** with maintained tooling (`cargo-ndk`, `xbuild`,
   the official Bevy mobile examples, `wasm-bindgen`/`trunk` for web).

### 4.3 Version and pinning policy

- Pin one Bevy minor (at adoption time, latest stable, e.g. `bevy = "0.16"` — verify at
  implementation) across all engine crates; upgrades are a dedicated ticket per Bevy
  release because the ecosystem moves with it.
- Every ecosystem crate is pinned to an exact version and recorded in the locked-deps
  table of 00-SPEC via the ADR. `cargo-deny` already gates licenses; everything chosen
  in Section 19 is MIT and/or Apache-2.0 (one zlib) so the wall holds.

---

## 5. High-level architecture

### 5.1 New crates and layering

Three crates, slotted into the existing L0–L5 model:

```
L0  bhippi-types            (+ engine ids, engine event variants — types only)
L1  bhippi-db               (+ engine repositories: scenes, builds, thumbnails)
L2  bhippi-engine           ← NEW  editor-domain library: scene model, transactions,
                                    undo, asset index, mind-map generator, BRP client,
                                    schema registry. No windowing, no rendering.
L2  bhippi-engine-build     ← NEW  build orchestration: target toolchains, packaging,
                                    signing, artifact ledger. Depends on bhippi-engine.
L3  bhippi-core             (unchanged; engine events ride the existing bus)
L4  bhippi-app              (+ engine command surface in commands.rs; owns the
                                viewport child process lifecycle)
S   bhippi-engine-viewport  ← NEW  a BINARY crate: the Bevy App. Runs the editor
                                    simulation and the game. Spawned by bhippi-app as a
                                    child process (Section 6). Also the template the
                                    shipped game is built from.
UI  ui/src/engine/          ← NEW  panels: Hierarchy, Inspector, Assets, Console,
                                    Toolbar, BuildPanel. Render + input only.
```

New dependency edges for `01-ARCHITECTURE §3.1` (via ADR-0020):

| From | To | Why |
|---|---|---|
| bhippi-engine | bhippi-types | ids, events |
| bhippi-engine | bhippi-db | persistence via repositories |
| bhippi-engine-build | bhippi-engine | reads project manifest/asset index |
| bhippi-app | bhippi-engine | command surface |
| bhippi-app | bhippi-engine-build | build commands |
| bhippi-engine-viewport | bhippi-engine (types-only feature) | shared scene schema |

`bhippi-engine-viewport` is deliberately a **leaf binary**, not a library other crates
import — it is the only crate allowed to depend on Bevy's windowing/rendering stack,
which keeps compile times for the main app sane and keeps wgpu/winit out of the Tauri
process entirely.

### 5.2 Process model

```
┌──────────────────────────────┐        ┌─────────────────────────────────┐
│  bhippi-app (Tauri process)  │        │  bhippi-engine-viewport (child) │
│                              │        │  Bevy App                       │
│  ┌────────────────────────┐  │  IPC   │  ┌───────────────────────────┐  │
│  │ bhippi-engine          │◄─┼────────┼─►│ EditorPlugin              │  │
│  │  • scene doc (truth)   │  │ BRP+   │  │  • render world           │  │
│  │  • transaction log     │  │ control│  │  • gizmos, grid, picking  │  │
│  │  • undo/redo stacks    │  │ channel│  │  • fly/orbit camera       │  │
│  │  • asset index         │  │(JSON-  │  │  • play-mode host         │  │
│  │  • mind-map generator  │  │ RPC on │  └───────────────────────────┘  │
│  └────────────────────────┘  │ 127.0. │  window embedded into the shell │
│  chat.rs ── engine actions   │ 0.1)   │  (Section 6)                    │
│  commands.rs ── UI IPC       │        │                                 │
└──────────────┬───────────────┘        └─────────────────────────────────┘
               │ typed IPC + coalesced events
        ┌──────▼───────┐
        │ ui/src/engine│  panels (hierarchy/inspector/assets/console/build)
        └──────────────┘
```

**Single source of truth:** the *scene document* lives in `bhippi-engine` inside the
Tauri process. The viewport is a *renderer and interactor* over that document. When the
user drags a gizmo in the viewport, the viewport sends a proposed transform delta over
the control channel; `bhippi-engine` applies it through the transaction system and
echoes the committed state back (to the viewport for redraw, and to the UI panels as a
coalesced event). This is the same "engine computes, views render" doctrine as the rest
of Bhippi, extended across a process boundary.

Rationale for child-process rather than in-process Bevy:
- wgpu/winit inside the Tauri process fights the webview event loop on all three OSes.
- A viewport crash (driver loss, shader bug) cannot take the shell down; the shell
  shows the pane's error state with a Relaunch button (C8).
- Play mode gets true process isolation from the editor UI — a runaway script can be
  killed like any other child, reusing the kill-switch tree.

### 5.3 Control channel

The shell ↔ viewport link is **JSON-RPC 2.0 over a loopback TCP socket** (bind
127.0.0.1, ephemeral port, token handshake passed via env at spawn — the same hygiene
as CLI provider spawning: explicit argv, scrubbed env, timeouts, INV-003).

Two logical services multiplexed on it:

1. **BRP-compatible surface** (`world.*`, `registry.*` methods) — provided by Bevy's
   own `bevy_remote` plugin in the viewport for *reads and play-mode debugging*.
2. **Editor service** (`editor.*` methods we define): `editor.load_scene`,
   `editor.apply_transaction`, `editor.set_gizmo_mode`, `editor.frame_selected`,
   `editor.screenshot`, `editor.set_camera`, `editor.begin_play`, `editor.end_play`,
   `editor.pick(x,y)`, `editor.drop_asset(asset_id, screen_pos)`.

All *writes* in edit mode go through `editor.apply_transaction` only — never raw BRP
mutation — so undo/redo and the audit trail can't be bypassed, by the human, the UI,
or the AI. In play mode, raw BRP mutation is allowed (it's a debugging live-world, the
edits are discarded on Stop, exactly like Unity play-mode edits).

### 5.4 Threading inside each process

- **Tauri process:** engine command handlers are async; anything heavier than a map
  lookup (scene save, asset scan, mind-map regen, thumbnail decode) goes to
  `spawn_blocking` (C7). One dedicated task owns the control-channel connection.
- **Viewport process:** Bevy's own schedule/thread-pool. The JSON-RPC listener runs on
  a Bevy async task and applies mutations via command queues at frame boundaries — no
  cross-thread world poking.

### 5.5 Event flow into the existing bus

`bhippi-types::events::Event` gains an `Engine(EngineEvent)` arm. EngineEvent variants
(coalesced by the existing coalescer):

```
SceneOpened { scene_id }              SelectionChanged { entity_ids }
SceneDirty { dirty: bool }            TransformsUpdated { batch }        // ≤20/s
HierarchyChanged { revision }         AssetIndexChanged { revision }
PlayStateChanged { state }            PlayStats { fps, entities, ms }    // ≤4/s
ConsoleLine { level, target, text }   BuildProgress { build_id, pct, step }
BuildFinished { build_id, ok, artifact_path }
ViewportStatus { alive, gpu_name }    EngineActionApplied { txn_id, by } // by: user|ai
MindMapRegenerated { revision }
```

The UI panels subscribe to these; nothing polls.

---

## 6. Viewport rendering strategy

The one genuinely hard integration problem: putting a real-time wgpu surface "inside"
a Tauri webview pane. Three strategies, in order of preference; the implementation
should land A, keep B as the portability fallback, and use C only for the Web build's
in-browser preview.

### Option A (primary): embedded native child window

The viewport process creates a borderless winit window; `bhippi-app` reparents it into
the shell window at the workbench pane's rectangle:

- **Windows:** `SetParent` + `WS_CHILD` on the winit HWND; resize via `SetWindowPos`
  driven by a pane-rect observer in the UI (the UI reports pane rect changes over IPC;
  Rust does the positioning — C1 respected: the UI reports facts, Rust acts).
- **macOS:** attach the child `NSView`/`NSWindow` via `addChildWindow` or view
  reparenting (Tauri exposes the raw NSWindow).
- **Linux/X11:** `XReparentWindow`; **Wayland:** no reparenting — fall back to Option B.

Input goes directly to the native window (best latency, correct mouse-capture for fly
camera and gizmo drags). The webview draws the panels *around* the rectangle; a 1px
hairline frames it so it reads as part of the design system.

This is the approach used in practice by DCC-adjacent tools and it delivers native
frame rates with zero pixel copies.

### Option B (fallback): shared-texture / streamed compositing

Where reparenting is unavailable (Wayland) or fails, the viewport renders offscreen
and the shell displays it:

- Preferred: OS shared handles (DXGI shared handle / IOSurface / dmabuf) into a
  `<canvas>` via the webview's GPU path where supported.
- Last resort: JPEG/turbo frames over the loopback socket at a capped 30fps with
  resolution scaling — same mechanism as the existing computer-use screenshot path, so
  the plumbing exists. Acceptable for editing, flagged in the UI as "compatibility
  presentation".

### Option C (web-preview only): WASM viewport in the Browser pane

The Web build target (Section 17) produces a wasm bundle. "Preview in Browser" serves
it on loopback and opens it in the existing BrowserView (loopback-only rule already
enforced there). This is a *play preview*, not the editing viewport.

**Decision gate:** Phase 1 (Section 24) ends with a spike proving Option A on Windows
(the primary dev OS per env), with B validated behind it. The ADR records the outcome.

---

## 7. Editor UI specification

### 7.1 Layout

Engine mode fills the workbench pane (or the maximized area) with a fixed arrangement
— no free-floating docking in v1 (docking systems are a tar pit; Unreal-style fixed
regions with collapsible panels cover 95% of use):

```
┌──────────────────────────────────────────────────────────────────────┐
│ Engine Toolbar (36px)                                                │
│ [▶ Play] [⏸] [■]   [Select|Move|Rotate|Scale]  [Grid ▾][Snap ▾]      │
│ [Camera ▾] [⛶ Maximize]                     [Scene: level_01 ● ]     │
├───────────────┬──────────────────────────────────┬───────────────────┤
│ HIERARCHY     │                                  │ INSPECTOR         │
│ (220px, coll.)│         3D VIEWPORT              │ (280px, coll.)    │
│ ▸ level_01    │   (native child window rect)     │ ┌───────────────┐ │
│   ▸ Environment│                                 │ │ Name  [Crate ]│ │
│     Sun        │                                 │ │ Tags  [prop  ]│ │
│     Sky        │                                 │ ├ Transform ────┤ │
│   ▸ Gameplay   │                                 │ │ pos x y z     │ │
│     Player     │                                 │ │ rot x y z     │ │
│     Crate  ◄sel│                                 │ │ scl x y z     │ │
│   ▸ Lighting   │                                 │ ├ MeshRenderer ─┤ │
│               │                                  │ │ mesh crate.glb│ │
│               │                                  │ │ mat  wood_01  │ │
│               │                                  │ ├ RigidBody ────┤ │
│               │                                  │ │ [+ Add comp.] │ │
├───────────────┴──────────────────────────────────┴───────────────────┤
│ CONTENT DRAWER / CONSOLE / BUILD  (tabbed, 180px, collapsible)       │
│ [Assets] [Console] [Build]                                           │
│  assets/ ▸ models ▸ props    [crate.glb][barrel.glb][lamp.glb] …     │
└──────────────────────────────────────────────────────────────────────┘
```

All chrome uses `tokens.css`; amber accent for selection; hairlines never shadows;
120/200ms motion; dark instrument aesthetic (04-PAGES design contract).

### 7.2 Toolbar

- **Play / Pause / Stop** — Section 16. Play flips the toolbar background tint (subtle
  amber wash) so play mode is unmistakable, and the Inspector shows a "play-mode edits
  are discarded" ribbon — the classic Unity footgun, pre-empted.
- **Transform tools:** `Q` select, `W` move, `E` rotate, `R` scale (Unreal/Unity
  standard). Space cycles gizmo space (world/local).
- **Grid & snap:** grid visibility, snap toggles + increments (move 0.1/0.5/1, rotate
  5°/15°/90°, scale 10%). Values persisted per project.
- **Camera menu:** perspective/ortho, top/front/side bookmarks, FOV, camera speed,
  "Frame Selected" (`F`).
- **Scene indicator:** current scene name + dirty dot; click = scene switcher; `Ctrl+S`
  saves (writes through `bhippi-engine`, never the text editor path).

### 7.3 Viewport interactions

| Interaction | Behaviour |
|---|---|
| RMB-hold + WASD/QE | fly camera (Unreal style), scroll adjusts speed |
| Alt+LMB / MMB / wheel | orbit / pan / dolly (Unity style) — both grammars supported |
| LMB click | pick entity (GPU picking in viewport; result → selection in engine) |
| LMB drag on gizmo | transform with snap; live coalesced `TransformsUpdated`; one undo entry per drag |
| Drag asset from Content Drawer into viewport | ray-cast drop: model instantiates at hit point (or 10m ahead if sky); drop is one transaction |
| `F` | frame selected; `Del` delete (transaction); `Ctrl+D` duplicate; `Esc` deselect |
| Box-select drag | multi-select |
| RMB tap (no move) | context menu: Add → (Empty, Cube, Sphere, Plane, Light ▸, Camera), Duplicate, Delete, Copy path for AI |

"Copy path for AI" copies the entity's stable path (e.g.
`level_01:/Gameplay/Crate#01J...ULID`) so a user can paste an unambiguous reference
into chat — a small feature that makes the human↔AI loop dramatically more precise.

### 7.4 Hierarchy panel

- Tree of the open scene's entities; parent/child = transform hierarchy.
- Drag to reparent (transaction), rename inline (`F2`), visibility eye and lock icons
  per row, type-ahead filter box, badge counts for hidden filtered children.
- **States (C8):** loading = skeleton rows; empty = "Scene is empty — right-click the
  viewport or ask the agent to block out a level"; error = fault card with Reload;
  populated = tree.

### 7.5 Inspector panel

- Renders the selected entity's components from the **reflection schema** the viewport
  registry exports (`registry.schema` over BRP) — the UI has zero hardcoded component
  layouts; it renders by field type (f32 → drag-number, Vec3 → triple, bool → toggle,
  enum → segmented control, asset handle → asset picker, color → swatch+picker).
- Multi-select shows common components; mixed values render as `—` and edit-applies to
  all (one transaction).
- **Add Component** searches the registry; **⋮** per component: remove, reset, copy as
  JSON (pasteable into chat for the AI).
- Every field edit is a transaction; drags coalesce into one undo step on release.
- States: loading skeleton; empty = "Nothing selected"; error = fault card; populated.

### 7.6 Content Drawer (asset browser)

- Left: folder tree mirroring `assets/` on disk. Right: thumbnail grid (64/96/128px
  zoom slider) with type badges (mesh/tex/mat/audio/scene/script/prefab).
- Thumbnails are generated by the viewport process (offscreen render for meshes,
  decode for textures) and cached in `.bhippi/engine/thumbnails/` keyed by content
  hash; served to the UI as data over IPC (small, cached).
- Actions: drag into viewport/hierarchy/inspector-slots; double-click scene = open;
  double-click script = open in **Editor mode** at that file (mode-switch handoff —
  the two workbench modes cooperate); RMB → Import…, New Folder, New Scene, New
  Script, New Material, Rename, Delete (confirm), Show in Explorer, Reimport.
- A file-system watcher (already available via the workbench watcher pattern) keeps
  the drawer live when files change on disk — including when the *AI writes assets via
  `<write_file>`*, which therefore "just works" for asset creation.
- States: loading = shimmer grid; empty = "No assets yet — drag files here or ask the
  agent to find free-licensed packs"; error; populated.

### 7.7 Console tab

- Structured log lines streamed from the viewport/play process (`ConsoleLine` events):
  level chips (trace/debug/info/warn/error), target (system/script/asset/build),
  monospace body, collapse-repeats counter, filter bar, Clear, Copy.
- Script panics and asset-load failures render as **FaultCard**-style entries with
  remedy buttons ("Open script at line", "Reimport asset", "Ask agent to fix" — the
  last one pre-fills the chat composer with the fault context: the single highest-value
  AI integration in the whole console).

### 7.8 Build tab

Section 17 covers the pipeline; UI summary: target cards (Windows / Linux / macOS /
Android / iOS / Web) each showing toolchain status (✓ ready / ⚠ missing pieces with a
"Fix" explainer), profile pick (Debug/Release), a Build button, live log stream,
artifact row with "Open folder" / "Run" / (Web) "Preview in Browser". Build history
list from the DB (Section 21).

### 7.9 Keyboard map (engine-pane focus)

`Q/W/E/R` tools · `F` frame · `Del` delete · `Ctrl+D` duplicate · `Ctrl+Z/Ctrl+Y`
undo/redo · `Ctrl+S` save scene · `Ctrl+P` play toggle · `F11` maximize ·
`Ctrl+1/2/3/4` bottom tabs · `Ctrl+'` cycles workbench mode (existing). All reachable,
AA contrast, reduced-motion respected (C8).

---

## 8. Game project layout on disk

Created by the New Game Project scaffold (a template embedded in `bhippi-engine`, not
downloaded):

```
<project root>/
├── Bhippi.game.toml            # game manifest (below)
├── assets/                     # everything the Content Drawer shows
│   ├── scenes/
│   │   └── level_01.bscn.json  # scene documents (Section 9)
│   ├── models/                 # .glb/.gltf (canonical), imported copies of FBX/OBJ
│   ├── textures/               # .png/.ktx2 (ktx2+zstd for shipping)
│   ├── materials/              # .bmat.json material definitions
│   ├── audio/                  # .ogg/.wav
│   ├── prefabs/                # .bprefab.json reusable entity templates
│   └── ui/                     # fonts, 9-slices, HUD layouts
├── scripts/                    # gameplay code (Section 10)
│   ├── Cargo.toml              # the game's Rust crate (rust track)
│   └── src/lib.rs
├── game/                       # runtime shell generated from bhippi-engine-viewport
│   └── (main.rs + platform glue; regenerated, user-editable marked regions)
├── builds/                     # output artifacts (gitignored)
└── .bhippi/
    ├── engine/
    │   ├── engine-map.json     # THE MIND MAP (Section 13)
    │   ├── asset-index.json    # ULID⇄path⇄hash table
    │   ├── thumbnails/         # content-hash keyed thumbs
    │   └── editor-state.json   # camera bookmarks, pane sizes, last scene
    └── rules.md                # existing project rules file (unchanged)
```

`Bhippi.game.toml`:

```toml
[game]
id = "01JC…ULID"
name = "My Game"
version = "0.1.0"
default_scene = "assets/scenes/level_01.bscn.json"
engine_track = "rust"            # "rust" | "scripted" (Section 10)

[render]
pipeline = "3d"                  # "3d" | "2d"
msaa = 4

[physics]
backend = "avian"
gravity = [0.0, -9.81, 0.0]

[targets.windows]
enabled = true
[targets.android]
enabled = false
package = "com.example.mygame"
min_sdk = 24
[targets.ios]
enabled = false
bundle_id = "com.example.mygame"
[targets.web]
enabled = true
canvas_fit = "window"
```

Everything human-readable, everything diffable, everything the AI can read and edit
with existing file tools — but scene/prefab/material edits *should* go through engine
actions (Section 14) so they are validated and undoable; the manifest and scripts are
plain files the normal `<write_file>` path handles.

---

## 9. Scene format, asset identity, and the data model

### 9.1 Identity

- **Every entity, asset, and scene gets a ULID** (the project-wide id convention).
- Assets: the ULID lives in a sidecar `<file>.meta.json` (Unity-style) written on
  import — so renames/moves on disk don't break references. The asset index maps
  ULID ⇄ relative path ⇄ blake3 content hash.
- Entities: ULID stored in the scene document; the *stable path*
  (`scene:/Parent/Child#ULID`) is a derived, human/AI-friendly address; ULID is the
  truth if names collide or change.

### 9.2 Scene document (`.bscn.json`)

Deterministic, sorted-key JSON (diff-friendly, AI-friendly). Shape:

```json
{
  "format": "bhippi-scene@1",
  "id": "01JC…",
  "name": "level_01",
  "settings": { "ambient": [0.02,0.02,0.03], "skybox": "asset:01JD…" },
  "entities": [
    {
      "id": "01JE…",
      "name": "Crate",
      "parent": "01JF…",
      "tags": ["prop", "physics"],
      "components": {
        "Transform":    { "pos": [4.0,0.5,-2.0], "rot": [0,0.383,0,0.924], "scale": [1,1,1] },
        "MeshRenderer": { "mesh": "asset:01JG…", "materials": ["asset:01JH…"] },
        "RigidBody":    { "kind": "dynamic", "mass": 20.0 },
        "Collider":     { "shape": { "cuboid": [0.5,0.5,0.5] } }
      }
    }
  ]
}
```

- Component payloads are the reflection-serialised forms; the schema (types, ranges,
  docs) is exported from the registry, so the format is *self-describing* — the same
  schema file the Inspector uses is published into the mind map for the AI.
- Load path: `bhippi-engine` parses/validates → sends to viewport via
  `editor.load_scene` → viewport instantiates through Bevy's scene machinery.
- Save path: engine document → deterministic serialise → atomic write (tmp+rename).
- **Prefabs** (`.bprefab.json`): same shape, one root; instances store
  `{ "prefab": "asset:…", "overrides": { per-entity component patches } }` — override
  semantics = JSON merge-patch, kept deliberately simpler than Unity's.

### 9.3 Compiled scenes for shipping

Builds compile `.bscn.json` → binary Bevy scene/asset packs (and textures → ktx2,
meshes → meshlet-friendly buffers where applicable) via `bhippi-engine-build`. JSON is
the *authoring* format only; runtime loads the compiled pack. This split is what lets
us keep authoring diffable without paying JSON parse cost on a phone.

---

## 10. Component model and scripting

### 10.1 Built-in component set (v1)

Transform (always), MeshRenderer, SkinnedMeshRenderer, Light (dir/point/spot),
Camera, RigidBody, Collider (box/sphere/capsule/mesh/heightfield), CharacterController,
AudioSource, AudioListener, AnimationPlayer, ParticleEmitter, NavAgent (Phase 7),
UiDocument (HUD), ScriptRef, Tag/Layer. Each is a Bevy component (mostly re-exports or
thin wrappers) registered with reflection + a `#[doc]` string — the doc strings surface
in both the Inspector tooltips and the mind-map schema, so writing docs once serves
human and AI.

### 10.2 Two scripting tracks

**Track A — Rust gameplay crate (`scripts/`), the primary track.**
The game's logic is a normal Rust crate exposing Bevy plugins/systems. The AI already
writes Rust well and Bhippi's whole toolchain is Rust. In-editor iteration uses
**hot-reload of the gameplay dylib** (adopt `dexterous_developer` or the
`bevy_simple_subsecond_system`/Dioxus-hot-reload lineage — evaluate at Phase 5;
fallback: fast full-restart of play mode, which the child-process design makes cheap:
kill + respawn + reload scene ≈ seconds). Ship builds link the crate statically.

**Track B — Rhai scripts, the sandboxed track.**
`bhippi-skills` already established Rhai as the sandboxed scripting stance. Adopt
`bevy_mod_scripting` (supports Rhai/Lua) so `.rhai` files in `scripts/` attach to
entities via `ScriptRef` with lifecycle hooks (`on_start`, `on_update(dt)`,
`on_collision`) and a curated, capability-limited API surface. Track B needs no
compiler on the user's machine, hot-reloads by file-watch trivially, and is the safer
default for AI-generated gameplay snippets. Projects choose a track in the manifest;
mixed mode allowed (Rust systems + Rhai behaviours).

### 10.3 Script errors

Compile errors (Track A) and runtime panics/exceptions (both tracks) flow into the
Console as faults with file/line and the "Ask agent to fix" affordance which injects
the error + relevant source into the chat composer.

---

## 11. Asset pipeline

### 11.1 Import matrix

| Input | Handling | Crate |
|---|---|---|
| .glb/.gltf | canonical; copy + meta; Bevy loads natively | bevy gltf |
| .fbx | convert → .glb at import via **ufbx** bindings (`ufbx` crate, MIT) on `spawn_blocking`; original kept in `assets/_source/` | ufbx |
| .obj | convert → .glb (tobj → glTF) | tobj |
| .png/.jpg/.tga/.exr/.hdr | copy + meta; ship-compile to ktx2+zstd | image, ktx2 |
| .wav/.ogg/.mp3 | copy + meta (mp3 transcoded to ogg at build) | symphonia/kira |
| .ttf/.otf | copy | bevy text |
| .blend | not parsed directly; document the Blender→glTF export path; optional auto-export hook if Blender is detected on PATH (never required) | — |

Import = (1) hash, (2) write meta with new ULID (or keep existing), (3) convert if
needed, (4) update asset index, (5) request thumbnail, (6) emit `AssetIndexChanged`,
(7) regenerate the mind map's asset section (debounced).

### 11.2 Licensing gate

Bhippi's product DNA is "no unlicensed image ships." The engine honours the same
spirit: every imported asset's meta has a `license` field (`unknown` by default).
The **build gate refuses Release builds containing `license = "unknown"` assets**
(Debug builds warn-list them in the Build tab; gates block on Release, C10). The
Content Drawer shows an amber corner-badge on unknown-license assets. The AI, when
asked to "find assets", is instructed (prompt file) to prefer CC0/CC-BY sources and to
write the license into the meta on import.

---

## 12. Editing semantics: transactions, undo/redo, autosave

Everything that mutates the scene document is a **Transaction**:

```rust
pub struct Transaction {
    pub id: Ulid,
    pub label: String,              // "Move Crate", "AI: block out arena"
    pub actor: Actor,               // User | Agent { session_id } | System
    pub ops: Vec<Op>,               // Spawn, Despawn, SetComponent, RemoveComponent,
                                    // Reparent, Rename, SetSceneSetting, InstantiatePrefab
    pub inverse: Vec<Op>,           // computed at apply time
}
```

- `bhippi-engine` validates ops against the schema, applies to the document, computes
  the inverse, pushes onto the undo stack (redo cleared), forwards to the viewport,
  emits events. **One code path for human, UI, and AI** — this is the load-bearing
  design decision of the whole plan.
- Undo/redo stacks are per-scene, capped (500 entries), and survive mode switches (not
  app restarts in v1).
- Gizmo drags / inspector slider drags coalesce: `begin_interactive` →
  stream deltas → `commit_interactive` = one transaction.
- **Autosave:** dirty scenes snapshot to `.bhippi/engine/autosave/` every 2 minutes
  and before Play; crash recovery offers restore on next open (error-state UX).
- Transactions are journaled to the DB (Section 21) — which gives the AI (and the
  user) an *audit trail*: "what did the agent change in this scene?" renders straight
  from the journal, and Review-Changes-style inspection of AI level edits becomes
  possible later.

---

## 13. The Engine Mind Map — how the AI sees the engine

The user asked for "a mind map for the engine so it's easy to find things." This is the
centrepiece of AI integration: a persistent, incrementally-updated, machine-readable
index at `.bhippi/engine/engine-map.json`, regenerated (debounced, on blocking pool)
whenever scenes/assets/scripts/settings change, and *summarised into the agent's
context* rather than dumped raw.

### 13.1 Structure

```json
{
  "format": "bhippi-engine-map@1",
  "revision": 412,
  "generated_at": "2026-08-29T10:11:12Z",
  "game": { "name": "My Game", "track": "rust", "default_scene": "level_01" },

  "scenes": [
    {
      "id": "01JC…", "name": "level_01", "path": "assets/scenes/level_01.bscn.json",
      "entity_count": 214, "summary": "Outdoor arena: terrain, 3 spawn points, 40 props, sun+sky",
      "roots": ["Environment", "Gameplay", "Lighting"],
      "outline": [
        { "path": "/Environment", "children": 52, "kinds": {"MeshRenderer": 50, "Light": 0} },
        { "path": "/Gameplay/Player", "components": ["Transform","CharacterController","ScriptRef(player.rhai)"] }
      ],
      "bounds": { "min": [-60,0,-60], "max": [60,25,60] },
      "spatial_digest": [
        { "cell": [0,0], "entities": 12, "notable": ["/Gameplay/Player", "/Environment/Fountain"] }
      ]
    }
  ],

  "assets": {
    "counts": { "mesh": 38, "texture": 61, "material": 14, "audio": 9, "prefab": 6 },
    "folders": ["models/props", "models/env", "textures/wood", "…"],
    "items": [
      { "id": "01JG…", "path": "assets/models/props/crate.glb", "kind": "mesh",
        "license": "CC0", "tris": 820, "used_by_scenes": ["level_01"] }
    ]
  },

  "scripts": [
    { "path": "scripts/src/player.rs", "kind": "rust",
      "systems": ["player_move", "player_jump"], "attached_to": [] },
    { "path": "scripts/player.rhai", "kind": "rhai",
      "hooks": ["on_update","on_collision"], "attached_to": ["level_01:/Gameplay/Player"] }
  ],

  "schema": {
    "components": [
      { "name": "RigidBody", "fields": { "kind": "enum(static|dynamic|kinematic)", "mass": "f32>0" },
        "doc": "Physics body simulated by Avian." }
    ]
  },

  "settings": { "physics": { "gravity": [0,-9.81,0] }, "targets_enabled": ["windows","web"] },
  "conventions": { "units": "meters", "up": "+Y", "forward": "-Z", "handedness": "right" },
  "recent_transactions": [
    { "id": "…", "label": "AI: placed 12 barrels along wall", "actor": "agent", "scene": "level_01" }
  ]
}
```

Key properties:

- **Hierarchical summarisation.** Scene `summary` strings and the `outline` (depth-2
  digest) let the agent orient without loading 214 entities; it drills down with
  queries (Section 14) only where needed. The `spatial_digest` (coarse XZ grid with
  notable entities per cell) gives the model *spatial* awareness — the thing LLMs are
  worst at — in text form.
- **Cross-references everywhere:** asset → scenes using it, script → entities it's on.
  "Find things to edit" becomes a lookup, not a search.
- **The schema section is generated from reflection**, so the AI always knows the
  exact legal fields, enums, and ranges for every component — malformed AI edits are
  rejected with the schema excerpt echoed back, enabling the existing one-repair-round
  pattern (`complete_json` precedent).
- Regeneration is incremental (per-scene sections rebuilt on their own dirty flags);
  a `revision` counter lets the agent detect staleness cheaply.

### 13.2 How it enters the agent's context

- On entering Engine mode or when the user's message mentions the game, `chat.rs`
  injects a **compact digest** (game line, scene list with summaries, asset counts,
  schema component *names* only — target ≤1.5k tokens) plus the instruction that full
  detail is available via `engine.query` actions. Never the whole map.
- `prompts/chat-engine.md` (versioned, C9) teaches: the coordinate conventions, the
  action grammar, the "query before you edit," "small transactions with clear labels,"
  and "screenshot after visual changes to verify" doctrines.

---

## 14. AI ↔ Engine interaction protocol

### 14.1 The action channel

Mirrors the proven inline-tag mechanism (`<write_file>`, `<computer_action>`): the
model emits `<engine_action>{json}</engine_action>`; `chat.rs` parses (relaxed JSON
tolerated, same as computer actions), routes to `bhippi-engine`, and streams the
result back into the turn. Actions:

**Read (never gated):**

```json
{ "op": "query_scene",   "scene": "level_01", "path": "/Gameplay", "depth": 2 }
{ "op": "query_entity",  "ref": "level_01:/Gameplay/Crate#01JE…" }
{ "op": "query_assets",  "kind": "mesh", "filter": "barrel" }
{ "op": "query_schema",  "component": "RigidBody" }
{ "op": "screenshot",    "camera": "editor|top|front|entity:<ref>", "annotate": true }
{ "op": "raycast",       "from": [0,10,0], "dir": [0,-1,0] }
{ "op": "measure",       "a": "<ref>", "b": "<ref>" }
```

`screenshot` with `annotate: true` overlays entity names/paths on the render — the
same visual-grounding trick as computer use, but with perfect labels because we *own*
the world. This pairs the model's vision with unambiguous references.

**Write (each becomes a Transaction with `actor: Agent`):**

```json
{ "op": "spawn",        "scene": "level_01", "parent": "/Environment", "name": "Barrel_07",
  "components": { "Transform": {"pos": [3,0,4]}, "MeshRenderer": {"mesh": "asset:01J…"} } }
{ "op": "instantiate",  "prefab": "asset:01J…", "at": [3,0,4], "parent": "/Environment" }
{ "op": "set",          "ref": "…#01JE…", "component": "Transform", "patch": {"pos": [5,0,-2]} }
{ "op": "add_component","ref": "…", "component": "RigidBody", "value": {"kind": "dynamic"} }
{ "op": "remove_component", "ref": "…", "component": "AudioSource" }
{ "op": "reparent",     "ref": "…", "new_parent": "/Gameplay" }
{ "op": "rename",       "ref": "…", "name": "Crate_Main" }
{ "op": "despawn",      "ref": "…", "recursive": true }
{ "op": "batch",        "label": "Block out arena walls", "ops": [ …up to 200 ops… ] }
{ "op": "create_scene", "name": "level_02" }
{ "op": "save_scene",   "scene": "level_01" }
```

**Control (gated):**

```json
{ "op": "play" } { "op": "stop" }
{ "op": "build", "target": "web", "profile": "debug" }
```

### 14.2 Permission model

Reuses `chat-permission-requested` / ActivityDock Allow-Deny verbatim (C10):

| Action class | Default |
|---|---|
| queries, screenshots, measure | always allowed |
| spawn / set / add_component / rename / instantiate | allowed while the session-level "Agent may edit scenes" toggle is on (on by default in Engine mode; every txn is undoable and journaled) |
| despawn, remove_component, batch > 20 ops, save_scene over dirty human edits | permission prompt with op summary ("Delete 14 entities under /Environment?") |
| play / stop | prompt first time per session, then allowed |
| build | always prompts (long-running, spawns toolchains) |

Every applied agent transaction emits `EngineActionApplied { actor: ai }` — the
ActivityDock shows it as a step ("✦ moved Crate_Main to (5, 0, −2)"), and the
hierarchy/viewport flash the touched entities with the amber accent for 800ms so the
human always *sees* what the AI just did. Trust is built by visibility.

### 14.3 Worked flows

**"Move the crate next to the fountain."**
1. Agent: `query_scene(filter: crate|fountain)` → gets both refs + positions.
2. `set Transform.pos` to a point 1.2m from the fountain (it computes from bounds in
   the reply — bounds come back with query results).
3. `screenshot(camera: editor, annotate: true)` → verifies visually → replies with the
   before/after and the undo hint ("Ctrl+Z reverts this").

**"Redesign this level as a canyon arena."**
1. Agent reads the mind-map digest + `query_scene(depth: 2)`, proposes a plan in
   prose, user approves.
2. Emits `batch` transactions in stages (terrain, walls, props, lights, spawns),
   ~50–150 ops each with clear labels — the ActivityDock shows staged progress, each
   stage is one undo step.
3. Screenshots from `top` and `editor` cameras between stages to self-correct.
4. Prompts "Play to test?" — the classic agentic loop, but in 3D.

**"Make the barrels explode when shot."**
1. Agent inspects `schema` + scripts section, writes `scripts/explosive.rhai` via
   normal `<write_file>`, then `add_component ScriptRef` on the barrels via `batch`.
2. `play`, watches Console events streamed into its context for script errors, fixes,
   `stop`.

### 14.4 Determinism and safety rails

- All writes are schema-validated; failures return the schema excerpt (one repair
  round, then surface a FaultCard).
- Refs are ULID-anchored — the agent can't "miss" due to a rename mid-conversation.
- A batch is atomic: any invalid op rejects the whole batch (gates block, C10).
- Rate limit: ≤5 transactions/second from the agent; larger work must batch (keeps
  the journal and event stream sane).

---

## 15. Chat, CLI, and slash-command integration

- **Slash commands** (added to the existing set in Chat.tsx, handled in Rust):
  - `/engine` — open workbench in Engine mode.
  - `/scene <name>` — open/switch scene.
  - `/play`, `/stop`.
  - `/build <target> [profile]` — e.g. `/build android release`.
  - `/enginemap` — dump the current mind-map digest into the chat (human-inspectable
    view of exactly what the AI sees — a debugging and trust feature).
- **Composer context chips:** when Engine mode is open, the composer shows a chip
  `⬡ level_01 · 3 selected` — the current scene and selection are injected into the
  agent's context automatically, so "make *these* smaller" resolves to the actual
  selection. (Selection refs ride the existing context-injection path.)
- **CLI providers:** CLIs Bhippi drives (claude/codex/etc.) run in the project
  directory (ADR-0013) and therefore see `Bhippi.game.toml`, `assets/`, `scripts/`,
  and `.bhippi/engine/engine-map.json` as plain files. `prompts/chat-workspace.md`
  gains a paragraph pointing external CLIs at the map file and at scene JSON for
  *read* orientation; writes from external CLIs land as file edits which the
  watcher picks up, validates, and (if a scene file changed outside a transaction)
  offers "Reload scene from disk" — external agents get correctness, in-app agents
  get the full transactional integration.
- **CliView:** unchanged; `bhippi engine` CLI verbs are not planned for v1 (the
  action channel + files cover it).

---

## 16. Play mode and simulation

- **Play** = the viewport process snapshots the current document, switches the Bevy
  App from Editor schedule to Game schedule (editor plugins/gizmos off, gameplay
  systems + scripts on, physics live, game cameras active). Same process in v1 —
  fastest iteration; the snapshot guarantees Stop restores the exact edit state.
- **Pause** freezes the game schedule (frame-step button appears: advance one frame).
- **Stop** restores the snapshot; play-mode mutations are discarded (with the
  Unity-footgun ribbon shown during play, §7.2).
- **Play stats** stream as `PlayStats` (fps, frame ms, entity count, draw calls) into
  the toolbar; the Console receives script logs live.
- While playing, the agent's queries and screenshots work against the live world
  (BRP direct), enabling "play and watch" flows: the agent can play the game, observe
  via screenshots + stats + logs, and report ("the player falls through the floor at
  the arena edge — the Collider on /Environment/Terrain doesn't cover x > 55").
- **Play in dedicated window** (secondary option): spawns the game as a separate
  process from the compiled gameplay crate — true "standalone run" without a build.

---

## 17. Build & deployment system

Owned by `bhippi-engine-build`. A build = `(target, profile)` → pipeline of steps,
each streaming `BuildProgress` events; all toolchain invocations are explicit-argv
child processes with scrubbed env and timeouts (the provider-spawn hygiene, INV-003).
Artifacts land in `builds/<target>/<version>/`, recorded in the DB (Section 21).

### 17.1 Shared steps (all targets)

1. **Preflight:** manifest validation, license gate (Release, §11.2), toolchain check.
2. **Asset compile:** scenes → binary packs, textures → ktx2+zstd, audio transcode,
   dead-asset elision (only assets reachable from enabled scenes ship).
3. **Codegen:** regenerate `game/` shell from the template with manifest values
   (window title, default scene, plugin list, target feature flags).
4. **Compile:** `cargo build` with target-specific flags below.
5. **Package + sign** (per target).
6. **Ledger:** hash, size, duration → DB; `BuildFinished` event.

### 17.2 Per-target specifics

| Target | Toolchain | Packaging | Notes |
|---|---|---|---|
| **Windows** | `cargo build --release` (msvc) | folder + zip; optional installer later | icon + version resource via `winresource`; primary dev target |
| **macOS** | cargo (aarch64/x86_64) | `.app` bundle via `cargo-bundle`; codesign/notarize if identity present (keychain, C11) | cross-build from Windows not supported — Build tab greys the card with "requires macOS" explainer |
| **Linux** | cargo (gnu) | tar.gz + .desktop file; AppImage later | |
| **Android** | NDK via **cargo-ndk** → `.so` per ABI (arm64-v8a, armeabi-v7a); Gradle wrapper project generated into `game/android/` → **apk/aab**; debug-keystore auto-generated, release keystore from keychain | apk (debug) / aab (release) | `GameActivity` glue via Bevy's android support; toolchain doctor checks ANDROID_HOME, NDK, JDK and offers guided fixes |
| **iOS** | cargo `aarch64-apple-ios` + generated Xcode project (via **xcodegen** template) or **xbuild** | .ipa | macOS-host-only, same greyed-card treatment on Windows; simulator target supported for quick checks on Macs |
| **Web/HTML5** | `cargo build --target wasm32-unknown-unknown` → **wasm-bindgen** → optional **wasm-opt**; generated `index.html` shell (canvas, loading bar, WebGPU-with-WebGL2-fallback) | static folder + zip | "Preview in Browser" serves on loopback into the existing BrowserView (§6 Option C); size budget reported (wasm size is the web pain point — the Build tab shows it prominently) |

### 17.3 Toolchain doctor

The Build tab's target cards run **doctor checks** (rustup target installed? NDK
found? JDK? wasm-bindgen version match?) with explicit remedy text and, where safe,
one-click fixes (`rustup target add …` behind a permission prompt). Doctor results
cache with a Recheck button. The AI can read doctor state via `query` and is
explicitly permitted to *explain* fixes but must prompt before running installers.

### 17.4 Build cancellation and concurrency

One build at a time per project (single-writer spirit); the kill switch cancels the
build's process tree via the existing cancellation-token pattern. Queued build
requests show in the ActivityDock queue like queued chat messages.

---

## 18. Runtime subsystem stack

| Subsystem | Choice | Why |
|---|---|---|
| Rendering | Bevy PBR (wgpu) | built-in; WebGPU/WebGL2/Vulkan/Metal/DX12 |
| Physics 3D | **Avian** (`avian3d`) | ECS-native, actively maintained, simpler integration than rapier's wrapper; deterministic-enough for v1 |
| Character control | Avian kinematic + `bevy-tnua` (optional) | tnua is the best-in-class character controller crate |
| Audio | **bevy_kira_audio** | mixing, tracks, spatial audio beyond bevy_audio |
| Animation | Bevy animation graph (glTF clips) | built-in since 0.14+ |
| UI in game (HUD) | Bevy UI | built-in; keeps stack uniform |
| Particles | **bevy_hanabi** | GPU particles, the ecosystem standard |
| Input | **leafwing-input-manager** | action-mapping abstraction → one input map works on desktop/mobile/web |
| Navmesh/AI | **oxidized_navigation** or **vleue_navigator** (Phase 7) | navmesh gen + pathfinding |
| Tweening | `bevy_tweening` | editor animations + gameplay ease |
| Gizmos (editor) | **transform-gizmo** (`transform-gizmo-bevy`) | proven translate/rotate/scale gizmo |
| Infinite grid (editor) | **bevy_infinite_grid** | the standard editor grid |
| Picking | Bevy's built-in picking (bevy_picking) | GPU/ray picking, built-in since 0.15 |
| Outline/selection highlight | `bevy_mod_outline` | selection visuals |
| Remote protocol | **bevy_remote** (BRP) | AI + tooling wire protocol |
| Scripting | **bevy_mod_scripting** (Rhai) | Track B (§10.2) |
| Hot reload (Track A) | `dexterous_developer` (evaluate) | dylib hot-reload for gameplay crate |

Everything above is MIT and/or Apache-2.0 licensed (verify at pin time via cargo-deny).

---

## 19. Open-source repositories to adopt

The explicit "don't code everything from scratch" shopping list. **Adopt** = Cargo
dependency. **Mine** = read/port patterns under license, credit in NOTICE.

| Repo | License | Use |
|---|---|---|
| `bevyengine/bevy` | MIT/Apache-2.0 | Adopt — engine core, scenes, reflection, BRP, picking, animation, UI |
| `Jondolf/avian` | MIT/Apache-2.0 | Adopt — physics |
| `nicopap/... / Onlypuppy7/transform-gizmo` (urholaukkarinen/transform-gizmo) | MIT/Apache-2.0 | Adopt — viewport gizmos |
| `ForesightMiningSoftwareCorporation/bevy_infinite_grid` | MIT/Apache-2.0 | Adopt — editor grid |
| `NiklasEi/bevy_kira_audio` | MIT/Apache-2.0 | Adopt — audio |
| `djeedai/bevy_hanabi` | MIT/Apache-2.0 | Adopt — particles |
| `Leafwing-Studios/leafwing-input-manager` | MIT/Apache-2.0 | Adopt — input actions |
| `makspll/bevy_mod_scripting` | MIT/Apache-2.0 | Adopt — Rhai scripting |
| `komadori/bevy_mod_outline` | MIT/Apache-2.0 | Adopt — selection outlines |
| `idanarye/bevy-tnua` | MIT/Apache-2.0 | Adopt — character controller |
| `ufbx/ufbx` (+ `ufbx-rust`) | MIT | Adopt — FBX import |
| `bbqsrc/cargo-ndk` | MIT/Apache-2.0 | Adopt (tool) — Android .so builds |
| `rust-mobile/xbuild` + `rust-mobile/android-activity` | MIT/Apache-2.0 | Adopt (tool/mine) — mobile packaging patterns |
| `rustwasm/wasm-bindgen`, `trunk-rs/trunk` | MIT/Apache-2.0 | Adopt (tool) — web builds |
| `jakobhellermann/bevy-inspector-egui` | MIT/Apache-2.0 | **Mine** — reflection→widget mapping logic informs our Inspector IPC schema renderer |
| `jakobhellermann/bevy_editor_pls` | MIT/Apache-2.0 | Mine — editor plugin architecture patterns |
| `rewin123/space_editor` | MIT | Mine — prefab/override design, editor UX decisions |
| `FyroxEngine/Fyrox` | MIT | Mine — editor interaction patterns (their editor is the most complete Rust reference) |
| `bevyengine/bevy_editor_prototypes` | MIT/Apache-2.0 | Mine — official editor-direction alignment; adopt pieces as they stabilise |
| `dexterous-developer` (lee-orr) | MIT/Apache-2.0 | Evaluate — hot reload |
| `KhronosGroup/glTF-Sample-Assets`, Kenney.nl packs, ambientCG | CC0/various | Starter-content pack for the New Game template (CC0 only) |

---

## 20. IPC command surface and event catalogue

New commands in `bhippi-app/src/commands.rs` (specta-generated to `ipc.ts`, C2).
Prefix `engine_` / `build_`:

**Project/scene:** `engine_status`, `engine_create_game_manifest`, `engine_open`,
`engine_close`, `engine_list_scenes`, `engine_open_scene`, `engine_new_scene`,
`engine_save_scene`, `engine_save_all`.

**Hierarchy/selection:** `engine_get_hierarchy(revision)`, `engine_select(ids)`,
`engine_get_selection`.

**Inspector:** `engine_get_entity(id)`, `engine_get_schema`,
`engine_apply_transaction(txn)`, `engine_begin_interactive`,
`engine_update_interactive`, `engine_commit_interactive`, `engine_undo`,
`engine_redo`, `engine_get_undo_stack`.

**Assets:** `engine_get_asset_tree`, `engine_import_assets(paths, dest)`,
`engine_get_thumbnail(asset_id)`, `engine_asset_ops(rename/move/delete/new_folder)`,
`engine_set_asset_license(asset_id, license)`.

**Viewport:** `engine_viewport_attach(rect)`, `engine_viewport_resize(rect)`,
`engine_viewport_relaunch`, `engine_set_tool(mode)`, `engine_set_snap(cfg)`,
`engine_set_camera(preset)`, `engine_frame_selected`, `engine_screenshot(opts)`.

**Play:** `engine_play`, `engine_pause`, `engine_step`, `engine_stop`.

**Mind map / AI:** `engine_get_mindmap_digest`, `engine_query(q)` (shared by UI
search and the agent action router).

**Build:** `build_get_targets`, `build_run_doctor(target)`, `build_start(target,
profile)`, `build_cancel(build_id)`, `build_history`, `build_open_artifact(id)`,
`build_preview_web(build_id)`.

Events: as listed in §5.5 (all coalesced through the existing bus, C3).

---

## 21. Database additions

New migration in `bhippi-db` (`0004_engine.sql`) + repositories (C4):

```sql
CREATE TABLE engine_games (           -- one row per project with a manifest
  id TEXT PRIMARY KEY, project_id TEXT NOT NULL, name TEXT NOT NULL,
  manifest_path TEXT NOT NULL, created_at TEXT NOT NULL);

CREATE TABLE engine_transactions (    -- the journal (audit trail, §12)
  id TEXT PRIMARY KEY, game_id TEXT NOT NULL, scene_id TEXT NOT NULL,
  actor TEXT NOT NULL,                -- 'user' | 'agent:<session_id>' | 'system'
  label TEXT NOT NULL, ops_json TEXT NOT NULL, created_at TEXT NOT NULL);
CREATE INDEX idx_engine_txn_scene ON engine_transactions(scene_id, created_at);

CREATE TABLE engine_builds (          -- artifact ledger (§17)
  id TEXT PRIMARY KEY, game_id TEXT NOT NULL, target TEXT NOT NULL,
  profile TEXT NOT NULL, status TEXT NOT NULL, started_at TEXT NOT NULL,
  finished_at TEXT, artifact_path TEXT, artifact_hash TEXT, size_bytes INTEGER,
  log_path TEXT);

CREATE TABLE engine_editor_state (    -- per-game UI prefs (camera, pane sizes)
  game_id TEXT PRIMARY KEY, state_json TEXT NOT NULL, updated_at TEXT NOT NULL);
```

Scene documents themselves stay **on disk** (they belong to the user's project and
must be diffable/committable); the DB holds Bhippi-side records only — consistent with
the workspace-files-vs-app-data split the project already practices.

---

## 22. Performance budgets

Extending the INV performance class (proposed INV numbers assigned in the ADR):

| Budget | Value | Measured by |
|---|---|---|
| Viewport frame rate, 1k-entity scene, editor mode | ≥ 55 fps (parity with the mind-map budget) | viewport stats harness |
| Gizmo drag → visible update | ≤ 16 ms (in-viewport, no IPC on the draw path) | manual + trace |
| Transaction apply → hierarchy/inspector event | ≤ 50 ms | integration test |
| Scene open (200 entities, warm assets) | ≤ 1.5 s | integration test |
| Engine mode cold attach (viewport spawn → first frame) | ≤ 3 s | integration test |
| Mind-map incremental regen (500 entities) | ≤ 200 ms on blocking pool | unit bench |
| Shell idle RSS increase with Engine mounted but idle | ≤ 60 MB (viewport process excluded — it's a child) | manual audit |
| Web debug build (template game) | ≤ 90 s on reference machine; wasm ≤ 25 MB before opt | build ledger |

---

## 23. Testing strategy

- **`bhippi-engine` unit tests:** transaction apply/inverse round-trip (property
  test: apply(txn); apply(inverse) == identity), scene serialise determinism
  (byte-identical re-save), prefab override merge, asset-index rename stability,
  mind-map digest token budget.
- **Schema conformance:** every registered component must reflect-serialise round-trip
  and appear in the schema export (test iterates the registry — catches "added a
  component, forgot reflection" forever).
- **Control-channel integration tests:** spawn a headless viewport (Bevy headless
  mode, no window) in CI; load scene, apply transactions, query back, screenshot to
  an offscreen target. This makes the whole editor-domain testable without a GPU
  window.
- **AI-protocol tests:** golden transcripts — action JSON in, transaction + events
  out; malformed action → schema-excerpt repair message.
- **Build pipeline:** Windows + Web targets built in CI for the template game (Android
  behind a nightly job once CI has the NDK); artifact hash recorded.
- **Architecture test:** new crate edges added to the enforced table (C6).
- **Manual QA checklist** appended to HANDOFF doc: reparent 3-level tree, undo across
  mode switch, drag-drop FBX import, play/stop restores state, kill viewport process
  mid-drag → error state → relaunch recovers.

---

## 24. Phased build order with tickets

Ticket prefix **ENG-**. Each phase is shippable and demoable; no phase starts before
its ADR/doc updates land (07-AGENT-GUIDE discipline).

**Phase 0 — Decision & scaffolding (1 sprint)**
- ENG-001 ADR-0020: engine workbench, crate edges, scope statement
- ENG-002 Crates scaffolded, architecture-test table updated, CI compiles them
- ENG-003 `Bhippi.game.toml` + New Game scaffold + Engine pill with empty state

**Phase 1 — Viewport spike (1–2 sprints) — HIGHEST RISK, DO FIRST**
- ENG-010 bhippi-engine-viewport binary: Bevy app, grid, fly camera, test cube
- ENG-011 Child-window embedding on Windows (Option A) + rect tracking + relaunch UX
- ENG-012 Control channel (JSON-RPC, token handshake), `editor.screenshot`
- ENG-013 Fallback Option B streamed presentation behind a flag
- **Gate:** 55fps embedded viewport resizing smoothly inside the workbench, or the
  ADR is amended with findings before proceeding.

**Phase 2 — Scene core (2 sprints)**
- ENG-020 Scene document model, `.bscn.json` load/save, ULID identity, meta sidecars
- ENG-021 Transaction system + undo/redo + journal table
- ENG-022 Hierarchy panel (4 states) + selection sync + picking
- ENG-023 Gizmos (transform-gizmo) + snap + interactive-transaction coalescing
- ENG-024 Inspector driven by reflection schema (core components)

**Phase 3 — Assets (1–2 sprints)**
- ENG-030 Asset index, import pipeline (glTF/textures/audio), meta + license field
- ENG-031 Content Drawer + thumbnails + drag-to-viewport instantiation
- ENG-032 FBX/OBJ conversion via ufbx/tobj
- ENG-033 Starter CC0 content pack in the New Game template

**Phase 4 — AI integration (1–2 sprints) — THE DIFFERENTIATOR**
- ENG-040 Mind-map generator + digest + `/enginemap`
- ENG-041 `<engine_action>` parse/route/permission integration, ActivityDock steps
- ENG-042 `prompts/chat-engine.md` + context chips (scene + selection injection)
- ENG-043 Annotated screenshots + query/raycast/measure ops
- ENG-044 Golden-transcript test suite

**Phase 5 — Play mode & scripting (2 sprints)**
- ENG-050 Play/pause/step/stop with snapshot restore, play stats, console stream
- ENG-051 Rhai track (bevy_mod_scripting) + ScriptRef + curated API + error faults
- ENG-052 Rust track: gameplay crate template + (evaluate) hot reload
- ENG-053 Physics (Avian) + character controller + input manager wiring

**Phase 6 — Build system (2 sprints)**
- ENG-060 bhippi-engine-build: shared pipeline, asset compile, ledger, Build tab
- ENG-061 Windows target end-to-end
- ENG-062 Web target + wasm shell + Preview in Browser + license gate
- ENG-063 Android target + toolchain doctor
- ENG-064 iOS/macOS targets (macOS-host gated), signing via keychain

**Phase 7 — Depth (ongoing)**
- ENG-070 Prefabs + overrides · ENG-071 Particles (hanabi) · ENG-072 Navmesh ·
  ENG-073 Animation graph UI · ENG-074 In-game UI/HUD editing · ENG-075 2D pipeline
  mode · ENG-076 Multi-scene additive editing

**Phase 8 — Hardening**
- ENG-080 Perf budgets measured + enforced · ENG-081 crash-recovery/autosave QA ·
  ENG-082 a11y pass on all panels · ENG-083 docs: 04-PAGES engine section,
  02-MODULE-CONTRACTS entries, 06-INVARIANTS additions, HANDOFF checklist

---

## 25. Risks and mitigations

| Risk | Severity | Mitigation |
|---|---|---|
| Native child-window embedding is flaky per-OS | High | Phase 1 is a dedicated spike with a hard gate; Option B streamed fallback always available; Windows-first matches the dev environment |
| Bevy minor releases churn the ecosystem | Medium | Exact pins; upgrades are dedicated tickets; leaf-binary isolation limits blast radius |
| Scope explosion ("a whole engine") | High | Phases are independently shippable; v1 explicitly excludes: terrain sculpting, visual shader editor, custom render features, multiplayer, console targets — each needs its own ADR |
| AI makes destructive scene edits | Medium | One transactional path, undo everything, journal + amber-flash visibility, permission gates on deletes/batches, rate limit |
| Compile times balloon for the workspace | Medium | Viewport is a leaf binary; Bevy behind it; `bhippi-engine` keeps Bevy behind a minimal `types-only` feature for the shared schema |
| iOS/macOS unbuildable from Windows | Certain | Honest greyed target cards with explainers; never fake it |
| Wasm size disappoints | Medium | wasm-opt + feature-trimmed Bevy + size shown prominently; Web is a preview/demo target first, not the flagship |
| Editor-in-a-≤900px-pane is cramped | Medium | Maximize mode (F11) is first-class from Phase 1 |
| License contamination from adopted crates/assets | Low | cargo-deny wall; per-asset license field + Release gate |

---

## 26. Glossary

| Term | Meaning |
|---|---|
| Engine mode | Third workbench mode next to Editor/Browser |
| Viewport process | `bhippi-engine-viewport` child process (Bevy app) embedded/streamed into the pane |
| Scene document | Authoritative in-memory scene owned by `bhippi-engine`, serialised as `.bscn.json` |
| Transaction | Validated, invertible, labelled batch of scene ops — the only write path (human, UI, or AI) |
| Engine Mind Map | `.bhippi/engine/engine-map.json` — machine-readable index of scenes/assets/scripts/schema the AI navigates |
| Engine action | `<engine_action>{json}</engine_action>` inline tag the chat agent emits; routed to transactions/queries |
| BRP | Bevy Remote Protocol — JSON-RPC world inspection/mutation, used for reads and play-mode debugging |
| Track A / Track B | Rust gameplay crate / sandboxed Rhai scripts |
| Doctor | Per-target toolchain readiness check in the Build tab |
| Stable path | Human/AI-readable entity address, `scene:/Parent/Child#ULID` |

---

*Next step if approved: write `docs/adr/0020-game-engine-workbench.md` from Sections
2–5 of this document, add the ENG-000-series rows to PROGRESS.md, and open Phase 0.*
