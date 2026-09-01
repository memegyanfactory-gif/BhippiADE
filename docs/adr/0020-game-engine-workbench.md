# ADR-0020 — Bhippi Game Engine Workbench (3D workflow)

- **Status:** Accepted
- **Date:** 2026-08-29
- **Derives from:** `planfor3dworkflow.md` (proposal), 00-SPEC-v1.0, 01-ARCHITECTURE, 04-PAGES, 06-INVARIANTS
- **Supersedes:** nothing (amends 01-ARCHITECTURE §3.1, §12; 02-MODULE-CONTRACTS; 06-INVARIANTS; 04-PAGES; 08-BUILD-ORDER; PROGRESS.md)
- **Relates to:** ADR-0012/0013 (project-first shell), ADR-0014 (workbench + activity dock), ADR-0016 (agent phases, typed faults), ADR-0018/0019 (computer use — the AI action-channel precedent)

## Context

The workbench has two modes — Editor and Browser (ADR-0014) — mounted as a split panel
next to chat. The owner asked for a third mode: **Engine**, a full game-engine editor in
the spirit of Unreal/Unity, serving two equal audiences: the **human** (manual 3D editing:
gizmos, viewport, inspector, hierarchy, asset browser, play-mode, multi-target builds) and
the **AI** (a structured, machine-readable protocol — an Engine Mind Map, typed engine
actions, annotated screenshots — so the chat agent can see, edit, and play the same world
through the identical transaction system the human uses).

The full design lives in `planfor3dworkflow.md` (scene format, asset pipeline, component
model, scripting tracks, build matrix, phase plan ENG-000-series). This ADR records the
load-bearing decisions: scope, the crate edges, the Bevy foundation, the process model, and
the invariants the workbench must obey. Where the plan conflicts with 00-SPEC,
06-INVARIANTS, or any prior ADR, those win.

## Scope statement

The engine is a **workbench capability of the ADE shell** — the same category of feature as
the code editor, the browser, and computer use: tools the coding agent uses to build things
for the user. The product's research/publishing domain lock (spec HARD REQ) is untouched;
the research pipeline (harvest → mind map → writer → publish) is not modified by this
change. The domain lock applies to the *research/publishing* pipeline, not to what the
coding agent can build in a workspace.

**v1 explicitly excludes** (each needs its own ADR before landing): terrain sculpting,
visual shader editor, custom render features, multiplayer/netcode, gamepad-only console
targets, and save-studio-style asset authoring (models/textures are imported, not authored).

## Decision

### 1. Three new crates, one new binary, one UI directory

Per plan §5.1, slotted into the existing L0–L5 model:

| Crate | Layer | Role |
|---|---|---|
| `bhippi-engine` | **L2** (new) | Editor-domain **library, no windowing/rendering**: scene document model, transaction system + undo, asset index, mind-map generator, schema registry, BRP read client, build preflight inputs. Holds the Bevy dependency behind a minimal `types-only` feature for the shared scene schema only. |
| `bhippi-engine-build` | **L2** (new) | Build orchestration: target toolchains, packaging, signing, artifact ledger. Depends on `bhippi-engine`. |
| `bhippi-engine-viewport` | **S** (new, leaf **binary**) | The Bevy App. Runs the editor simulation and, in play mode, the game. Spawned by `bhippi-app` as a child process. **Only** crate allowed to link Bevy's windowing/rendering stack. Also the template the shipped game is built from. |
| `ui/src/engine/` | **L5** (new) | Panels: Hierarchy, Inspector, Assets (Content Drawer), Console, Build tab, Toolbar. Render + input only. |

New dependency edges for `01-ARCHITECTURE §3.1` (enforced by `tests/architecture.rs`, C6):

| From | To | Why |
|---|---|---|
| bhippi-engine | bhippi-types | ids, engine events |
| bhippi-engine | bhippi-db | persistence via repositories |
| bhippi-engine-build | bhippi-engine | reads project manifest / asset index |
| bhippi-app | bhippi-engine | command surface, viewport lifecycle |
| bhippi-app | bhippi-engine-build | build commands |
| bhippi-engine-viewport | bhippi-engine (`types-only` feature) | shared scene schema |

`bhippi-engine-viewport` is a **leaf binary**, not an importable library, so wgpu/winit
never enter the Tauri process and the main app's compile times stay sane (plan §5.1, §25).

### 2. Engine core is Bevy 0.19 (pinned), not a from-scratch renderer

- **Pin:** `bevy = 0.19.1` verified against the ecosystem at adoption: `transform-gizmo-bevy
  0.11` (bevy ^0.19), `avian3d 0.7` (^0.19), `bevy_kira_audio 0.26` (^0.19),
  `leafwing-input-manager 0.21` (^0.19), `bevy_mod_scripting 0.21` (^0.19),
  `bevy_hanabi 0.19` (^0.19) — Section 19 of the plan listed them; every one is now on
  0.19 or has a known fallback.
- **Temporary gap, recorded:** `bevy_infinite_grid 0.18` still targets bevy ^0.18. Until a
  ^0.19 release exists, the editor grid is an in-repo generated grid mesh in
  `bhippi-engine-viewport` (a leaf concern, ~100 lines). Adoption is re-checked each Bevy
  upgrade ticket; the in-repo fallback costs nothing if the crate never catches up.
- Ecosystem crates are exact-pinned and recorded in this ADR; `cargo-deny` already gates
  licenses (everything adopted is MIT/Apache-2.0, one zlib). Any new crate needs an ADR.
- Bevy minor upgrades are a **dedicated ticket** (the ecosystem moves with Bevy). Editors
  and shipped runtime are the same Bevy `App` — no editor approximation drift.

### 3. Process model: child process, engine-process-owned truth

The sane split from plan §5.2–5.4:

- **`bhippi-engine` (in the Tauri process) owns the scene document, the transaction log,
  the undo/redo stacks, the asset index, and mind-map generation.** Writes to the document
  go through one `apply_transaction` path for human, UI, and AI (plan §12).
- **`bhippi-engine-viewport` is a child process** rendered into the workbench pane. It is a
  *renderer and interactor*: gizmo drags propose deltas over the control channel; the engine
  applies them and echoes state back.
- **Control channel:** JSON-RPC 2.0 over a loopback TCP socket (127.0.0.1, ephemeral port,
  token handshake via env at spawn — the INV-003 spawn hygiene). Two services multiplexed:
  a `world.*`/`registry.*` surface (Bevy Remote Protocol reads + play-mode debugging) and an
  `editor.*` surface we define (`load_scene`, `apply_transaction`, `set_gizmo_mode`,
  `frame_selected`, `screenshot`, `set_camera`, `begin_play`, `end_play`, `pick`,
  `drop_asset`).
- **Write discipline:** in edit mode all writes flow only through
  `editor.apply_transaction` — never raw BRP mutation — so undo/redo and the audit journal
  cannot be bypassed by human, UI, or AI. In play mode raw BRP mutation is allowed
  (Unity-style, discarded on Stop).
- **Viewport rendering:** Option A (native child-window embedding — Windows `SetParent` /
  `WS_CHILD`; macOS child view; X11 reparent) is primary; Option B (streamed shared-texture /
  JPEG-turbo fallback over loopback) ships behind a flag; Option C (WASM viewport in the
  Browser pane) is web-preview only. Phase 1 closes a spike proving Option A on Windows.

Why a child process rather than in-process Bevy: wgpu/winit fight the webview event loop;
a viewport crash (driver loss, shader bug) must not take the shell down (C8 error state with
Relaunch); play mode gets true process isolation from the editor UI.

### 4. AI integration is transactional, permissioned, and visible

- Engine Mind Map at `.bhippi/engine/engine-map.json` (plan §13): machine-readable,
  incrementally regenerated index of scenes/assets/scripts/schema/settings, summarised into
  the agent's context as a ≤1.5k-token digest; full detail via `engine.query` actions.
- Inline `<engine_action>{json}</engine_action>` channel (plan §14) routed by `chat.rs`
  through the same transaction system as the human. Every write is a journaled,
  undoable Transaction with `actor: Agent`; deletes/batches/builds go through the existing
  `chat-permission-requested` Allow-Deny flow (C10); nothing silently mutates.
- The AI doctrine lives in a versioned prompt file `prompts/chat-engine.md` (C9/INV-035).

### 5. Invariants added (this ADR amends 06-INVARIANTS)

| ID | Class | Invariant |
|---|---|---|
| INV-070 | A | Scene writes only via `bhippi-engine::Transaction` (human, UI, or AI); plays in edit mode bypassing `apply_transaction` are impossible by construction |
| INV-071 | A | Every applied transaction is journaled to `engine_transactions` with actor and label; undo/redo and "what did the agent change?" render from the journal |
| INV-072 | A | Viewport is a child process; a kill/restart lands in the pane's error state, never in the shell's |
| INV-073 | A | Webview computes nothing for the engine: scene state, picking math, gizmo math, undo stacks, asset scanning are all in `bhippi-engine` (extends INV-051's spirit to L2) |
| INV-074 | S | Release builds containing assets with `license = "unknown"` **fail**; debug builds warn-list them (gates block; extends the no-unlicensed-image rule to assets) |
| INV-075 | A | Engine panels implement the loading/empty/error/populated state floor and keyboard reachability (invoked INV-034 for `ui/src/engine/`) |
| INV-076 | P | Engine events (transform batches, play stats, build progress, thumbs) coalesce through the existing ≤20/s bus (invoked INV-021); the 3D viewport itself never redraws over IPC |

### 6. Database and IPC

- New `bhippi-db` migration `0004_engine.sql` + repositories (C4): `engine_games`,
  `engine_transactions` (the audit journal), `engine_builds` (artifact ledger), and
  `engine_editor_state` (per-game UI prefs). Scene documents stay **on disk** — they belong
  to the user's project and must be diffable/committable.
- New `engine_*` / `build_*` commands in `bhippi-app/src/commands.rs` (plan §20), all specta
  generated to `ui/src/lib/ipc.ts` (INV-032). No business logic in TypeScript (R3).

## Consequences

- **Easier:** the human↔AI loop is precisely grounded (stable entity paths, annotated
  screenshots, audit trail); editor and shipped game cannot drift (same Bevy App); the
  existing permission, undo, audit, and event-bus machinery extends to 3D work instead of a
  parallel system.
- **Harder:** a Bevy 0.19 leaf binary adds a large one-time compile and keeps wgpu/winit
  toolchain requirements on Windows/MSVC on the build path; native child-window embedding is
  per-OS fragile (Option B streamed fallback exists); the ecosystem pin wall must be
  re-checked on every Bevy release. iOS/macOS target builds are macOS-host-only and are
  honestly greyed elsewhere (never faked).
- **Docs that change in this ADR's change:** `01-ARCHITECTURE` §3.1 table + §12 layout +
  layer diagram, `02-MODULE-CONTRACTS` (M13 engine / M14 engine-build / M15 engine-viewport),
  `06-INVARIANTS` (tables above), `04-PAGES` (A1f Engine sections), `08-BUILD-ORDER`
  (ENG series), `PROGRESS.md`.

## Alternatives

- **Godot embedded (gdext):** C++ engine, its own editor process/scene format/GDScript
  culture; embedding the editor is impractical; headless-driving gives the worst of both
  worlds. Interop idea only (glTF exchange).
- **Fyrox:** serious Rust alternative but retained-mode scene graph, weaker mobile story,
  smaller ecosystem. Mined for editor UX patterns, not adopted.
- **three.js/Babylon viewport with a Rust backend:** drags scene logic into TS or forces a
  chatty IPC render loop; violates the "webview computes nothing" rule in spirit and cannot
  be the shipping runtime for native/mobile builds.
- **Write our own on wgpu:** multi-year effort; the owner explicitly asked to reuse existing
  repos.
- **In-process Bevy:** rejected for the process-model reasons in Decision 3.