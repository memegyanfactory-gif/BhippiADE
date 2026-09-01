# 13 — Engine AI-Control & Unreal-Grade Usability Plan

**Owner goal (stated intent):** the AI must be able to *fully control* the engine; the engine
must *look and work like Unreal*; anything the AI generates — levels, maps, materials,
objects, HUD, camera, layout — must be editable by hand afterwards; the HUD must be its own
file with real widget options (text, buttons, images, bars); opening **Main** and pressing
**Play** must actually run the game inside the engine so it can be tested.

**Status:** authored and implementation-reconciled 2026-09-01 · **the plan is complete; the
implementation is not** · Phase 0 core complete (107/108 closure work remains) · Phase 1
core complete (114 blocked on a provider-contract ADR; 110/115/116/117 have explicit
remainders) · Phase 2 core complete (124 conversion remainder) · Phase 3 runtime complete
(134/136/139 editor/migration remainders) · Phase 4 partial · Phase 5 core complete
(165/166/167 remainders) · Phase 6 complete · Phase 7 complete · Phase 8 partial · ticket
range `ENG-100…ENG-399` · Phase 9 started (ADR-0032 and the first `/gamedebug` slice;
ENG-201/206/207/208 remain partial) · Phase 10 (quality improvement) and Phases 11–12
(runtime sandboxing) are specified and not yet complete · Phases 13–24 now define the
minimal editor reset, capability registry and Unreal-class expansion track; none is
implemented merely because it is specified here.

**Authority:** this document sits below `docs/00-SPEC-v1.0.md`, `docs/06-INVARIANTS.md`,
`docs/01-ARCHITECTURE.md`, `docs/02-MODULE-CONTRACTS.md` and the ADRs. Where a task needs to
change one of those, the row says so and names the ADR to write **first**.

---

## 0. How to use this document

### 0.1 Checkbox discipline

```
- [ ]  not started
- [~]  in progress (put your agent name + date on the line)
- [x]  done — code AND tests exist AND the group's Acceptance line is provable
- [!]  blocked — say what blocks it, on the same line
```

**A falsely ticked box is the only unforgivable thing in this repo.** A `[ ]` next to
shipped-looking UI is honest; a `[x]` next to a stub is not. Every group ends with an
**Acceptance** line — that is what makes the tick legal.

This document being "complete" means every remaining ticket has a bounded implementation
contract, named seams, verification evidence and an exit condition. It does **not** turn an
unfinished implementation into `[x]`. Only the evidence named on the ticket may do that.

### 0.2 Working rules for every task here

1. One phase group per change. Do not start Phase 5 rendering and Phase 6 physics together.
2. Every scene mutation goes through `bhippi-engine::EngineTransaction` (INV-070). No
   exceptions — not for the UI, not for the AI, not for "just this one save".
3. No business logic in TypeScript (INV-073). If you are computing a transform, merging a
   scene, generating an id, or validating a payload in `.tsx`, you are in the wrong crate.
4. No `unwrap()` / `expect()` outside tests (the workspace lints deny both).
5. No SQL outside `bhippi-db`.
6. Gates block, they never warn.
7. Update §12 (Progress log) at the end of every session with what actually shipped.

### 0.3 Ticket numbering

`ENG-100…ENG-399` is reserved for this plan. Where a task completes or supersedes an existing
ticket from `docs/08-BUILD-ORDER.md` (ENG-010, ENG-020, ENG-030, ENG-040…), the row says so;
tick the old ticket there too.

---

## 1. Audit baseline — what existed when the plan was opened (verified 2026-09-01)

Everything here was read out of the tree, not assumed. It is the **before** snapshot that
explains F1–F9; it is not a second progress tracker. The checkbox, shipped and closure
sections in §4 are the current state and supersede baseline wording such as "there is no".

### 1.1 The chat system, and exactly how it touches the engine

`crates/bhippi-app/src/chat.rs` is the conversational engine (ADR-0006).

| Piece | Where | What it does |
|---|---|---|
| Turn loop | `ChatEngine::run_turn` | streams provider deltas; emits `ChatDelta` / `ChatThinking` / `ChatTool` / `ChatTurnDone` to the webview |
| System prompt assembly | `chat.rs` ≈2160–2260 | `CHAT_SYSTEM` + workspace context + rules + **`engine_context(&workspace)`** |
| Engine doctrine | `prompts/chat-engine.md` (v4 at baseline; v9 at reconciliation), pulled in as `ENGINE_SYSTEM` | tells the model the Unreal layout, the play rules, the `<engine_action>` tag |
| Live scene facts | `engine_context()` — `chat.rs:3996` | calls `query_scene_in_workspace` → injects scene path, entity count, mind-map digest |
| Action extraction | `extract_engine_action_tags()` — `chat.rs:4017` | string-scans the **finished** assistant text for `<engine_action>…</engine_action>` |
| Action application | `chat.rs:2653–2685` | per tag → `crate::engine::apply_action_in_workspace` → `ToolAction::EditEngine` card → `EngineSceneChanged` event |
| UI reaction | `EngineView.tsx` `events.engineSceneChanged.listen` | calls `reload()` — re-reads the whole scene from disk |

**The current end-to-end path, in one line:**
user prompt → model writes prose containing `<engine_action>{json}</engine_action>` → turn
finishes → app scrapes tags → each becomes one transaction against the **default** scene on
disk → file rewritten → event → the Engine pane throws away its state and re-reads the file.

### 1.2 The engine domain — `crates/bhippi-engine` (pure and headless)

| Module | Lines | Substance |
|---|---|---|
| `document.rs` | 488 | `SceneDocument` (`bhippi-scene@1`), `Entity`, `SceneSettings`, `SceneKind{main,level,hud,empty}`, validation (unique ids, parents exist, cycle detection), `stable_path` / `resolve_ref`, lenient ULID upgrade |
| `transaction.rs` | 1 067 | `Op` (9 ops), `EngineTransaction` with captured inverse, `Session` (interactive multi-op), `UndoStack` (cap 500) — **the single write path** |
| `action.rs` | 453 | `EngineAction` (9 kinds) lowered into `Op`s |
| `schema.rs` | 538 | component registry: 19 components, typed fields, range/enum/asset-ref validation, every error carries a hint |
| `api.rs` | 1 079 | `SceneQueries` — 13 deterministic read queries, compact/deep (ADR-0027) |
| `asset.rs` | 398 | `AssetIndex`, `AssetKind` (13 kinds), `LicenseState`, `used_by_scenes` |
| `scaffold.rs` | 648 | New Game file plan: `main` + `hud` + `level_01`; 7 spawn templates |
| `manifest.rs` | 295 | `Bhippi.game.toml` — `default_scene`, `hud_scene`, `levels[]`, render/physics/targets |
| `mindmap.rs` | 258 | the digest text handed to the AI |
| `query.rs` | 221 | hierarchy projection + find-by-name |

### 1.3 The app seam — `crates/bhippi-app/src/engine.rs` at baseline

17 Tauri commands: `get_engine_status`, `engine_create_game_manifest`, `engine_query_scene`,
`engine_apply_action`, plus 13 `engine_query_*` wrappers over `SceneQueries`.
`apply_action_in_workspace()` is shared by IPC and chat: load file → rewrite name refs to
ids → parse `EngineAction` → `into_ops` → `EngineTransaction::apply` → `doc.dump()` → write.

### 1.4 The editor UI — `ui/src/engine/` at baseline

`EngineView.tsx` (shell, toolbar, Play), `EngineViewport.tsx` (Three.js: OrbitControls,
TransformControls, `ViewHelper` axis widget, fly camera, picking, weather),
`EngineHierarchy.tsx` (flat Outliner), `EngineInspector.tsx` (Transform + material map
paths), `EngineContentDrawer.tsx` (file listing), `EngineSceneDocument.ts` (types **and
logic**).

### 1.5 Viewport / runtime crates

At baseline, `bhippi-engine-viewport` contained an unused JSON-RPC `editor.*` design plus a
13-line Bevy stub. ADR-0028 later retired the child-process renderer and removed the stub;
the shipping viewport and play runtime now live in `ui/src/engine/`. The protocol remains as
an explicitly unused design. `bhippi-engine-build` owns build orchestration and gates.

---

## 2. Findings baseline — the nine things that blocked the goal

Each was verified in the baseline tree. The phases in §4 exist to close exactly these; do
not read the present-tense consequence column as current status after its closing phase.

| # | Finding | Evidence | Consequence |
|---|---|---|---|
| **F1** | **Two write paths.** The UI mutates React state and writes the whole scene file with `api.writeFile`; the AI goes through `EngineTransaction`. | `EngineView.tsx` `commitDoc()` + `handleSaveScene()` vs `engine.rs::apply_action_in_workspace` | Breaks INV-070. AI edits and hand edits clobber each other; `EngineSceneChanged` triggers `reload()` and silently discards the user's unsaved buffer; two unrelated undo stacks exist (a React `historyRef` array and the Rust `UndoStack`). |
| **F2** | **The journal is dead.** `engine_projects` / `engine_journal` exist in `0004_engine.sql`; **zero** Rust code writes them. | `grep -rn "engine_journal" crates` → 0 hits | Breaks INV-071. No "what did the agent change?", no cross-session undo, no crash recovery, no AI action history. |
| **F3** | **Business logic in TypeScript.** Scene creation, weather application, duplication, ULID generation, scene merging, kind inference all live in `.ts`. | `EngineSceneDocument.ts`: `createDefaultEntity`, `applyWeather`, `duplicateEntity`, `newEntityId`, `mergeScenes`, `inferKind` | Breaks INV-073 and forks the truth: TS writes `mesh: "cube"` where `schema.rs` demands `asset:<ulid>` or `""`; `mergeScenes` rewrites ids to `level_01J…`, which is not a ULID and fails `SceneDocument::parse`. |
| **F4** | **The AI vocabulary is too small and is scraped from prose.** 9 single-entity actions, one scene, applied only *after* the turn ends. | `action.rs::EngineAction`; `chat.rs:2653` | The AI cannot create or delete a scene, set weather/skybox, register a level, create a material, import an asset, edit the HUD, move the camera, select an entity, or press Play. It receives no per-action result, so there is no read→act→verify loop and no repair round. |
| **F5** | **Play mode is a placeholder.** A RAF loop finds the first entity whose name contains "player", moves it with WASD, and applies `velY -= 0.009`. | `EngineViewport.tsx:667-723` | No scripts, no colliders, no camera possession, no HUD interaction, no win/lose, no level travel. "Press Play and test the game" does not exist. |
| **F6** | **The HUD has no document format.** HUD widgets are entities carrying `UiDocument { layout: "health" }` — a magic string — rendered in play mode as a `<div>` printing the entity name. | `scaffold.rs::hud_scene`; `EngineViewport.tsx:771-787` | The user cannot change a button's text, a bar's colour, an anchor, or a font, because none of those fields exist anywhere. |
| **F7** | **No content generation.** The AI can only *reference* assets. `.mat.json` and `.shader.json` have no schema, no parser, no validation. | `asset.rs` indexes file names; nothing parses their contents | "Generate materials and use them properly" is impossible; the AI can only invent paths, which `chat-engine.md` then correctly forbids. |
| **F8** | **The viewport does not render the real scene.** `MeshRenderer.mesh` is treated as a primitive name; a `.glb` renders as a grey box; `MaterialOverride` maps are stored and never applied. | `EngineViewport.tsx:485-515`; `grep -c albedo EngineViewport.tsx` → 0 | What the AI generates is not what the user sees, so neither can verify anything. |
| **F9** | **Hierarchy is not transform-accumulated.** Children are positioned in world space — the code says so in a comment. | `EngineViewport.tsx` — "parent hierarchy is logical, not transform-accumulated in this preview" | Moving a parent does not move its children. Prefabs, rigs and grouped level pieces cannot work. |

---

## 3. Target architecture

### 3.1 The one-write-path seam (fixes F1, F2, F3)

```
        user drags a gizmo            AI emits an action           @-command / palette
                │                            │                            │
                ▼                            ▼                            ▼
        ┌──────────────────────────────────────────────────────────────────────┐
        │  bhippi-app::engine::EngineSessions   (in-memory, per project+scene)  │
        │  open documents · dirty flags · UndoStack · selection · play state    │
        └──────────────────────────────────────────────────────────────────────┘
                │  every mutation, no exception
                ▼
        ┌──────────────────────────────────────────────────────────────────────┐
        │  bhippi-engine::EngineTransaction::apply  (INV-070, captures inverse) │
        └──────────────────────────────────────────────────────────────────────┘
             │                      │                        │
             ▼                      ▼                        ▼
      scene file on disk     engine_journal (INV-071)   EngineEvent bus (≤20/s, INV-076)
                                                              │
                             ┌────────────────────────────────┼───────────────────┐
                             ▼                                ▼                   ▼
                      Outliner / Details              Three.js viewport      chat tool result
                      (render from event)             (patch, no reload)     (fed to the model)
```

The webview becomes a pure renderer of engine state. It never computes a transform, never
generates an id, never merges a scene, never decides what a component means.

### 3.2 The AI bridge (fixes F4)

Two layers, both landing on the same transaction path:

- **Read** — `SceneQueries` (already built, ADR-0027) plus project / asset / console / play
  readers, exposed to the model as *retrieval*, never as a whole-project dump.
- **Write** — `EngineAction` grown from 9 to ~40 verbs, always submitted as an
  `EngineActionBatch { label, actions[] }` that becomes **one** transaction and therefore
  **one** undo step ("Undo AI Change"), with a per-action typed result envelope returned to
  the model so it can repair itself without a human in the loop.

### 3.3 The three document families the user hand-edits

| File | Format id | What the user edits by hand | Editor surface |
|---|---|---|---|
| `assets/scenes/*.bscn.json` | `bhippi-scene@1` | levels, Main, entities, transforms, components | Outliner + Details + viewport gizmos |
| `assets/ui/*.hud.json` | `bhippi-hud@1` **(new)** | text, buttons, bars, images, anchors, colours, fonts, click actions | HUD canvas mode + HUD Details |
| `assets/materials/*.mat.json` | `bhippi-material@1` **(new)** | PBR maps, tint, roughness, emissive, shader ref | Material Details + preview sphere |

All three are deterministic, sorted-key, diffable JSON that a human and a model can both read.

---

## 4. The plan

Phases are ordered by dependency, not by glamour. Phases 0 and 1 are load-bearing for
everything the owner asked for; do not skip them to build a nicer Outliner first.

---

### Phase 0 — One write path, one truth  ·  `ENG-100…109`

*Closes F1, F2, F3. Nothing else in this document is safe until this lands.*

- [x] **ENG-100** — `EngineSessions` state in `bhippi-app`: an `Arc<Mutex<…>>` map of
      `(project_root, scene_rel) → OpenScene { doc, undo: UndoStack, dirty, selection }`.
      Loads on first touch; every read command serves from it; no command re-reads the file
      behind the session's back.
- [x] **ENG-101** — New commands, all going through `EngineTransaction`:
      `engine_open_scene`, `engine_close_scene`, `engine_save_scene`, `engine_save_all`,
      `engine_undo`, `engine_redo`, `engine_history` — replacing the React `historyRef`.
- [x] **ENG-102** — `engine_begin_session` / `engine_record` / `engine_commit_session`
      wrapping `transaction::Session` so a gizmo *drag* is **one** undo step, not one per
      frame (the interactive-coalescing contract in `transaction.rs`). *Shipped and tested;
      the Three.js gizmo does not yet call it because `TransformControls` already fires
      once on `mouseUp` — the API is there for per-frame tools (Phase 4 sculpt/paint).*
- [x] **ENG-103** — Journal writes (INV-071): a `bhippi-db` `EngineRepo` with
      `upsert_project`, `append_transaction` (monotonic `revision`), `list_journal`,
      `journal_since`. Every committed transaction writes one row: actor, label, ops JSON.
      *Completes the long-open `ENG-020`.*
- [x] **ENG-104** — Crash recovery: on open, if journal revision > the file's recorded
      revision, offer replay; autosave every N seconds into `.bhippi/engine/autosave/`.
      *Shipped as a stronger write-on-every-transaction snapshot: a new process offers
      Recover / Discard, recovery stays dirty until Save, and the authored file is untouched.*
- [x] **ENG-105** — Delete the TypeScript logic (INV-073). `EngineSceneDocument.ts` keeps
      **types only**, and those types come from the regenerated `ipc.ts` bindings.
      Remove: `createEmptySceneDoc`, `createStarterSceneDoc`, `createDefaultEntity`,
      `duplicateEntity`, `newEntityId`, `applyWeather`, `mergeScenes`, `inferKind`,
      `stripJsonComments`.
- [x] **ENG-106** — Rewrite `EngineView.tsx` as a controlled view over engine state: it
      dispatches commands and renders `EngineSceneState` events. No `commitDoc`, no local
      scene mutation, no `api.writeFile` on a scene.
- [~] **ENG-107** (codex, 2026-09-01 — touched-object patching and revision coalescing ship; commit-to-projection browser timing remains) — Granular events instead of `reload()`: `EngineSceneChanged` grows into
      `EngineTransactionApplied { scene, txn_id, actor, label, touched, ops }` so the viewport
      patches only the touched entities (INV-076 coalescing, INV-079 ≤50 ms).
      *Done:* `EngineSceneChanged` now carries `txn_id`/`actor`/`label`/`touched`/
      `entity_count`/`dirty`/`revision`, and the pane re-reads the **live session** instead
      of the file, so an agent edit no longer discards unsaved work. *Not done:* the
      viewport still rebuilds the whole entity group per state push rather than patching
      only `touched` — that lands with the Phase 5 renderer, and INV-079 is unmeasured.
- [~] **ENG-108** (codex, 2026-09-01 — read-only two-way Diff ships; saved-base merge and per-hunk apply remain) — Conflict rule when a scene file changes on disk under an open dirty
      session: show a fault card with *Keep mine / Take disk / Diff*; never silently discard.
      *Done:* `EngineSceneState.disk_conflict` (a length+mtime stamp taken at every read and
      write), a notice bar with **Keep mine** / **Take disk**, and `engine_reload_scene`.
      The notice provides **Keep mine**, **Take disk**, and a read-only side-by-side **Diff**
      sourced from the live session and disk without mutating either copy.
- [x] **ENG-109** — Tests: an AI action and a UI action on the same scene interleave without
      loss; undo after an AI batch restores exactly; journal rows match applied transactions;
      a CI grep asserts no scene-shaped write happens in `ui/`.

**Acceptance:** with the Engine pane open and dirty, an AI edit lands and the user's unsaved
work survives; `Ctrl+Z` undoes the AI's whole batch; `engine_history` lists both actors;
`sqlite3 … "select count(*) from engine_journal"` is non-zero.

---

### Phase 1 — The AI ↔ Engine bridge  ·  `ENG-110…119`

*Closes F4. This is §76 of `engine plan.md` — "the most important system".*

- [~] **ENG-110** (claude, 2026-09-01 — every scene-scoped verb ships; the rest is blocked on
      later phases, see below) — Grow `EngineAction`. Keep the existing 9; add (grouped):

      Scene/level    create_scene · delete_scene · rename_scene · set_scene_settings
                     set_weather · set_skybox · set_ambient · register_level
                     set_default_scene · set_hud_scene · reorder_levels
      Entity         spawn_prefab · spawn_mesh · set_tags · set_visible · set_locked
                     group_entities · align_entities · distribute_entities
      Components     (existing add/patch/remove) · set_component_property (dotted path)
      Assets         import_asset · create_material · set_material · create_shader
                     create_texture · create_prefab · apply_prefab
      HUD            hud_add_widget · hud_set_prop · hud_remove_widget
                     hud_reparent_widget · hud_set_rect  (see Phase 3)
      Editor         select · focus · set_camera · set_view_mode · set_gizmo
      Play           play · pause · step · stop · possess
      Scripts        create_script · attach_script · set_script_config


      **Shipped (23 verbs, all lowering to transactions):** `translate` · `look_at` ·
      `set_component_property` (dotted path) · `set_tags` · `set_visible` · `set_locked` ·
      `set_mesh` · `set_material` · `attach_script` · `group_entities` · `align_entities` ·
      `distribute_entities` · `set_weather` · `set_scene_settings`, plus the original nine.
      New `Op::SetTags` and `Op::SetSettings`; new `Visibility` component in the registry.
      The engine computes the quaternion for `look_at` and the centroid for `group_entities`
      so the model never does geometry.

      **Deliberately not shipped, because the thing they would write does not exist yet:**
      `import_asset` / `create_material` / `create_shader` / `create_texture` /
      `create_prefab` (Phase 2 owns `bhippi-material@1` and the import pipeline) ·
      `create_scene` / `delete_scene` / `register_level` / `set_default_scene` (Phase 2) ·
      every `hud_*` verb (Phase 3 owns `bhippi-hud@1`) · `play` / `pause` / `step` /
      `possess` and `create_script` (Phase 6 owns the runtime; a `.rhai` file nothing can
      run is a fake capability) · `select` / `focus` / `set_camera` / `set_view_mode` /
      `set_gizmo` (these drive the editor, not the document — they need the ENG-107 event
      path the viewport does not consume yet). `prompts/chat-engine.md` §4 tells the model
      exactly this list, so it says "I can't yet" instead of inventing a file.

- [x] **ENG-111** — `EngineActionBatch { label, scene, actions[] }` → **one**
      `EngineTransaction`, one journal row, one undo step. Partial failure rolls the whole
      batch back and returns which action failed and why.
- [x] **ENG-112** — A typed result envelope per action:
      `{ ok, action_index, entity?, asset?, message, hint?, schema_excerpt? }`, so a rejected
      payload teaches the model the correct shape in the same turn.
- [x] **ENG-113** — **Apply during the turn, not after.** Parse `<engine_action>` /
      `<engine_batch>` out of the *streaming* delta (mirror `run_computer_turn`), apply, and
      feed the result envelope back as the next model input — the read→plan→act→verify loop
      of `engine plan.md` §80. Keep post-turn scraping as a fallback for one release.
- [!] **ENG-114** BLOCKED (claude, 2026-09-01) — Native tool-calling path for providers that support it: expose the same
      verbs as JSON-schema tools generated from the Rust types (`specta`), so the tag
      protocol is the compatibility path, not the primary one.
- [x] **ENG-115** (codex, 2026-09-01) — Read tools for chat: wire the existing 13 `engine_query_*` commands into
      the model's tool surface, plus `get_project_info`, `get_selection`, `get_console`,
      `get_play_stats`, `search_assets`. Retrieval only — never dump the project.
      The tag-compatible query bridge now covers every projection plus bounded project,
      console, play-stat and text/kind-filtered asset retrieval. Console rows cap at 200;
      one answer caps at 40 rows.
- [~] **ENG-116** (claude, 2026-09-01 — modes, gate and plan card ship; no inline Edit) — Permission + preview (`engine plan.md` §82): three modes — *Ask*,
      *Auto*, *Autonomous*. In Ask mode a batch renders as a plan card
      (`+18 entities · ~WorldSettings · −OldSky · 7 imports`) with Approve / Reject / Edit,
      reusing the existing `ChatPermissionRequested` machinery.
- [~] **ENG-117** (claude, 2026-09-01 — the journal is the stream; typed subsystem events await their subsystems) — Engine event stream to the model (§83): `entity_created`,
      `scene_loaded`, `asset_imported`, `material_created`, `shader_compiled`,
      `script_compiled`, `game_started`, `runtime_error`, `build_completed` — the same bus
      the UI listens on, summarised into the turn.
      *Shipped:* the turn's system prompt now carries the last six journal rows with their
      actor and label, plus the user's live selection and whether the scene is dirty — so
      the model can tell its own edits from the user's and does not redo work. *Not shipped:*
      typed per-subsystem events; `shader_compiled`, `script_compiled`, `game_started`,
      `runtime_error` and `build_completed` have no subsystem to fire them yet (Phases 2, 5,
      6). Inventing the event names before the subsystems exist would be the fake-breadth
      this plan is against.
- [x] **ENG-118** — `prompts/chat-engine.md` (currently **v9**): rewritten around the tool surface, the
      batch envelope, the HUD format, the material format and the verify loop. Version the
      file and note the bump in `PROGRESS.md`.
- [x] **ENG-119** — Golden transcripts (`ENG-044`): fixture conversations that must produce
      byte-identical scene output — "build me a 3-room level", "make the sky stormy and dim
      the sun", "add a health bar to the HUD", "the player is floating, fix it".

**Acceptance (revised, honestly):** the original line assumed Phase 2 and Phase 3 had
already landed — a model cannot create a material or a HUD widget while neither format
exists, and could not have on the day this was written. What is provable now, and is covered
by `crates/bhippi-app/tests/engine_batches.rs`: in one turn the model reads the scene, spawns
and places entities, groups and aligns them, aims a light, changes the weather and adjusts a
component — as **one** journal batch that `Ctrl+Z` reverses completely; a batch that fails
anywhere writes nothing and comes back with the failing index and that component's real
schema. The material/HUD/play half of the original line moves to Phase 2/3/6 acceptance,
where the formats it needs are actually built.

---

### Phase 2 — Real content generation  ·  `ENG-120…128`

*Closes F7. "Generates levels and maps and materials and objects and uses them properly."*

- [x] **ENG-120** — `bhippi-material@1` format + `material.rs` in `bhippi-engine`:
      parse / validate / dump `assets/materials/*.mat.json`.

      ```json
      {
        "format": "bhippi-material@1",
        "id": "01J…", "name": "crate_wood",
        "shader": "assets/shaders/pbr_standard.shader.json",
        "maps": { "albedo": "asset:01J…", "normal": null, "roughness": null,
                  "metallic": null, "ao": null, "emissive": null },
        "params": { "base_color": [0.72,0.55,0.32], "roughness": 0.68, "metallic": 0.0,
                    "emissive": [0,0,0], "emissive_strength": 0.0,
                    "tiling": [1,1], "offset": [0,0], "alpha_mode": "opaque", "double_sided": false }
      }
      ```

- [x] **ENG-121** — `bhippi-shader@1` format + validation for `assets/shaders/*.shader.json`
      (file-based, assignable — *not* a node graph; the node graph stays out of scope per
      ADR-0020 and would need its own ADR).
- [x] **ENG-122** — Asset writes go through the transaction path too: `create_material`,
      `create_shader`, `create_texture` produce a transaction whose inverse deletes the file,
      so "Undo AI Change" also removes generated assets.
- [x] **ENG-123** — Asset meta + licence sidecars (`*.meta.json`): source, hash, licence,
      import settings. `LicenseState::Unknown` blocks a Release build (INV-074) — gate, not
      warning. *Feeds `ENG-030`.*
- [~] **ENG-124** (claude, 2026-09-01 — import ships; mesh *conversion* deferred, see below) — Import pipeline: GLB/GLTF pass-through, OBJ via `tobj`, FBX via `ufbx`,
      all normalised to GLB in `assets/models/`, with unit/axis correction and a reported
      bounding box. Textures normalised to PNG/KTX2 with sRGB flags.
- [x] **ENG-125** — `bhippi-prefab@1`: a named entity subtree + overrides, instantiable by
      the AI and by drag-drop; edits to the prefab propagate to instances that have not
      overridden the field. *Feeds `ENG-070`.*
- [x] **ENG-126** (codex, 2026-09-01) — Deterministic procedural helpers the AI can call with a seed:
      `grid_layout`, `scatter`, `room_from_bounds`, `corridor_between`, `perimeter_walls`,
      `stack`, `ring`. Same seed ⇒ same scene, asserted by test.
- [x] **ENG-127** — Generated-content provenance (§98): every entity and asset records
      `created_by: user|agent`, `txn_id`, `at`. The Outliner can filter "AI-generated"; the
      Details panel shows the origin; the AI can find and clean up its own output.
- [x] **ENG-128** — Content gates that block:
      every asset ref resolves · no unknown component or field · no unlicensed asset in a
      Release build · `default_scene` is Main · every `levels[]` path exists · the HUD path on
      Main exists · weather id ∈ the eight presets. Same rules the AI is told in
      `chat-engine.md` — but enforced in Rust, not in a prompt.

**Acceptance:** "make me a warehouse level with crates and a wet concrete floor" produces
real `.mat.json` files, real scene entities that reference them by id, a viewport that shows
them, and a Release build that refuses to ship if a texture's licence is unknown.

*Provable now* (`crates/bhippi-app/tests/engine_batches.rs`,
`crates/bhippi-engine-build/src/lib.rs`): the material file is written and validates, the
mesh references it, one Ctrl+Z removes both, a failed batch rolls its files back, `scatter`
lays out a reproducible field of crates in one action, and a build **fails** on a missing
level, a bad weather id or a dangling asset reference. *Not provable*: "a viewport that shows
them" — the renderer still does not read materials (F8, Phase 5), so nothing here claims the
result looks right.

---

### Phase 2 — what shipped, precisely

| Ticket | Shipped | Deliberately not |
|---|---|---|
| **ENG-120** material | `bhippi-engine/src/material.rs` — `MaterialDocument`, six fixed PBR slots, typed params, validation that **refuses** out-of-range values instead of clamping (a material quietly rewritten under the user is worse than one that says what is wrong). Emissive is unclamped: it is radiance, not albedo. | — |
| **ENG-121** shader | `ShaderDocument` with `stage` and a `.wgsl` `source` that must exist. **Also fixed:** the scaffold had been writing `lit_pbr.mat.json`/`lit_pbr.shader.json` as hand-written constants wearing the `@1` format markers *while predating the formats* — so the first parser to exist would have rejected a new game's own starter material. Both now come from the real types, plus a real `lit_pbr.wgsl` for the document to point at. | Node graphs — still ADR-0020's exclusion. |
| **ENG-122** asset writes on the transaction path | `engine/content.rs` — `ContentAction` produces a `FileChange` carrying the bytes written **and the bytes that were there before**, so the inverse is exact. Content steps ride in the same batch as scene actions; the session keeps a file ledger keyed by transaction id beside the engine's `UndoStack`, which is what lets `bhippi-engine` stay filesystem-free while "create a material and put it on the floor" is one Ctrl+Z. A batch that fails after writing rolls the file back. | **Stated boundary:** `UndoStack` skips a transaction with no scene ops, so an *asset-only* change is applied and journaled but is **not** on the undo stack — where Unreal also leaves "create asset". Pairing creation with the assignment that uses it puts both on one step. Tested both ways. |
| **ENG-123** sidecars + licence | Every generated asset writes a sidecar naming its origin; `set_asset_license` records a licence **keeping the asset's existing ULID** (its identity across renames); `import_file` writes one at import, honestly `unknown` when unstated. **Real hole closed:** the scaffold wrote its own scenes and assets with no sidecar, so a brand-new project could not produce a Release build — INV-074 blocked it on its own starter content. | — |
| **ENG-124** import | `content::import_file` — copy in, route by extension, write the sidecar so the asset has an id and a licence state the moment it lands. | The **conversion** half. OBJ→GLB and FBX→GLB need `tobj`/`ufbx` plus a unit/axis normalisation pass and a GLB writer. Copying a file while claiming to have converted it would be worse than not offering it. **Needs its own ADR for the dependency choice before any code.** |
| **ENG-125** prefabs | `prefab.rs` — capture a subtree, instantiate with fresh ULIDs per copy, a `PrefabInstance` marker recording source + overrides, propagation that skips overridden components and **never** propagates `Transform` (per-instance by definition; propagating it would teleport every copy onto the prefab's authored position). | Propagation into descendants — needs a stable local-id trail the format does not carry, and doing half of it silently would read as a bug. |
| **ENG-126** procedural | `procedural.rs` (pinned SplitMix64) plus seven callable verbs: the original scatter/grid/ring/perimeter/stack and `room_from_bounds`/`corridor_between`. Rooms split oriented, thickness-aware wall cuboids around validated door/window openings; corridors compute open-ended walls between exact room-boundary points. Architectural lowering uses seed/input-derived ULIDs, is byte-identical for identical inputs, rejects overlapping rooms/openings, and retains the 4096 cap. | — |
| **ENG-127** provenance | Every spawn is stamped with a `Provenance` component naming the actor, the transaction id and the timestamp. Stamped in `commit_ops`, the one place the transaction id exists — which also means a new spawning verb cannot forget to opt in. | The Outliner "AI-generated" filter chip and the Details-panel display (UI, Phase 4). |
| **ENG-128** gates | `gates.rs` — `check_project` (default scene, level list, HUD path, weather id, per-scene validity) and `check_assets` (dangling references found by walking the whole payload, because `MeshRenderer.materials` is an array and `Collider.shape` is free-form JSON; plus licence state, warning in Debug, blocking in Release). **Wired where it blocks:** the build calls both and fails on any blocker, and `engine_check_content` exposes the same report to the UI. | — |

**Bug found by the gates, in the build crate:** `collect_tree` matched scene files on
`extension == "bscn"`, but they are `*.bscn.json`, whose extension is `json` — so **no scene
had ever been collected**, and the structural validation pass in `collect` had been running
over an empty list since the crate was written. Fixed, and the gates now receive the scenes
paired with their relative paths so a finding can name its file.

---

### Phase 3 — The HUD system  ·  `ENG-130…139`

*Closes F6. This is the owner's explicit "there is a specific HUD file with all the options —
text, buttons and everything — and the user can change what the AI generated."*

- [x] **ENG-130** — `bhippi-hud@1` document format, `assets/ui/*.hud.json`, parsed and
      validated in `bhippi-engine::hud`:

      ```json
      {
        "format": "bhippi-hud@1",
        "id": "01J…",
        "name": "hud_main",
        "canvas": { "reference": [1920, 1080], "scale_mode": "fit", "safe_area": 0.04 },
        "widgets": [
          {
            "id": "01J…", "name": "HealthBar", "kind": "progress_bar",
            "parent": null, "order": 0, "visible": true, "locked": false,
            "rect": { "anchor": "top_left", "offset": [32, 32], "size": [280, 22], "pivot": [0, 0] },
            "style": { "bg": "#00000080", "fill": "#e0483c", "fg": "#ffffff",
                       "radius": 6, "border": { "width": 1, "color": "#ffffff30" },
                       "padding": [6, 8], "opacity": 1.0, "font": "asset:01J…", "font_size": 14 },
            "bind": { "value": "player.health", "max": "player.health_max" },
            "props": { "show_text": true, "format": "{value}/{max}" }
          },
          {
            "id": "01J…", "name": "PauseButton", "kind": "button",
            "parent": null, "order": 1, "visible": true, "locked": false,
            "rect": { "anchor": "top_right", "offset": [-32, 32], "size": [96, 34], "pivot": [1, 0] },
            "style": { "bg": "#151922cc", "fg": "#e8eaf0", "radius": 8, "font_size": 15 },
            "props": { "text": "PAUSE", "align": "center", "hover_bg": "#1e2531ee",
                       "disabled": false, "tooltip": "Pause the game" },
            "on_click": { "action": "pause_game" }
          }
        ]
      }
      ```

- [x] **ENG-131** — Widget catalog + schema registry (same pattern as `schema.rs`, so the
      Details panel and the AI schema excerpt come from one source):

      | kind | key props |
      |---|---|
      | `panel` | layout (`free`/`row`/`column`), gap, scroll |
      | `text` | text, align, wrap, max_lines, uppercase |
      | `button` | text, icon, hover/pressed/disabled style, `on_click` |
      | `image` | source (asset ref), fit (`contain`/`cover`/`stretch`), tint |
      | `progress_bar` | value/max bind, direction, show_text, format |
      | `crosshair` | style, size, spread bind |
      | `icon_row` | source, count bind, spacing (lives, ammo) |
      | `timer` | bind, format (`mm:ss`), count_down |
      | `minimap` | source camera, zoom, shape |
      | `joystick` | side, dead_zone, mobile-only |
      | `key_prompt` | action ref, auto-resolves to the bound key/button |
      | `list` | bind (array), item template, max_rows |

- [x] **ENG-132** — `on_click` / `on_press` action vocabulary:
      `pause_game`, `resume_game`, `stop_game`, `quit_to_main`, `load_level(name)`,
      `set_var(name, value)`, `toggle_widget(id)`, `call_script(script, fn)`.
- [x] **ENG-133** — Binding model: `bind` paths resolve against runtime state
      (`player.health`, `score`, `level.name`, `time.remaining`, plus any script-exposed
      var). Unresolved bindings render the placeholder in the editor and log **once** at
      runtime — never spam, never crash.
- [~] **ENG-134** (codex, 2026-09-01 — canvas drag/resize, 8px snapping and z-order ship; alignment and multi-select remain) — HUD editor mode: opening a `.hud.json` (or double-clicking **hud** in the
      Content Browser) switches the viewport into a **2D canvas**: reference-resolution
      frame, anchor guides, safe-area overlay, drag/resize handles, snapping, alignment
      tools, z-order (`Bring Forward` / `Send Back`), multi-select.
- [x] **ENG-135** — HUD Details panel: every property of the selected widget as a real
      control — text field, font picker, size stepper, colour pickers (bg/fg/fill/border),
      radius, padding, opacity, anchor 3×3 grid, offset/size numeric fields, visibility,
      lock, binding field with autocomplete, `on_click` action dropdown with its arguments.
- [x] **ENG-136** (codex, 2026-09-01) — HUD Outliner: the widget tree, drag to reparent and reorder, per-widget
      eye/lock, filter box.
      A drop onto a container reparents; a drop onto a leaf places the widget with that
      leaf's siblings. Reparent+reorder is one validated undo step.
- [x] **ENG-137** — HUD actions for the AI (`hud_add_widget`, `hud_set_prop`,
      `hud_remove_widget`, `hud_reparent_widget`, `hud_set_rect`), on the same transaction
      path, so an AI-built HUD is undoable and hand-editable in the same panel.
- [x] **ENG-138** (claude, 2026-09-01 — completed by Phase 6's runtime) — Runtime HUD renderer for play mode: a real widget renderer over the
      viewport, anchored/scaled to the reference resolution, bindings live, buttons
      clickable and firing their actions.
- [~] **ENG-139** (codex, 2026-09-01 — deterministic converter ships; preview/approval and recoverable Main update remain) — Migration off `UiDocument { layout: "health" }`: `hud.bscn.json` becomes
      `hud_main.hud.json`; `hud_scene` in the manifest points at it; the loader upgrades the
      old shape once, deterministically, the way `parse_lenient` upgrades ids.
      Legacy scene/entity ids become the HUD/widget ids, known layouts become typed widgets,
      the old source remains recoverable, and a repeat-conversion test asserts byte identity.

**Acceptance:** the AI generates a HUD; the user opens the HUD file, changes the pause
button's text to "MENU", drags it to the top-left, recolours the health bar, and presses Play
— the running game shows exactly those changes.

*Provable now:* the HUD is a real `bhippi-hud@1` document with twelve widget kinds, each with
a typed, validated field list; the Engine pane has a **HUD** tab with a widget tree, a canvas
preview and a Details form generated from the engine's own schema; changing the pause
button's text, anchor, colours or click action and pressing Play shows exactly those changes,
because Play reads the *live session* rather than the last save. `hud_action` tests cover the
text change, the anchor move and the recolour end to end.

*Still required to close the editor ticket:* direct drag/resize on the canvas,
drag-to-reparent/reorder in the HUD Outliner, and a deterministic upgrader for projects that
already contain `hud.bscn.json`. Live values and button actions are no longer a remainder:
Phase 6 supplies them through the disposable play runtime.

---

### Phase 3 — what shipped, precisely

| Ticket | Shipped | Deliberately not |
|---|---|---|
| **ENG-130** format | `bhippi-engine/src/hud.rs` — `bhippi-hud@1`: canvas (reference resolution, scale mode, safe area), widgets with anchor/offset/size/pivot rects, style, bindings, props and a click action. Duplicate ids, missing parents, parent cycles, non-container parents and absurd safe areas are all refused. | — |
| **ENG-131** widget catalog | Twelve kinds — panel, text, button, image, progress_bar, crosshair, icon_row, timer, minimap, joystick, key_prompt, list — each with a typed `PropSchema` list. `widget_schema()` is the single source the Details panel, the Add menu and the AI's schema excerpt all render from, so they cannot drift. | — |
| **ENG-132** click actions | A closed `WidgetAction` list: pause/resume/stop, quit to Main, load level, set var, toggle widget, call script. Closed on purpose — an action the runtime cannot perform is a button that silently does nothing, which is worse than a rejected document. | Wiring them to a runtime (Phase 6). |
| **ENG-133** bindings | `bind` slots per widget with `binding_paths()` reporting everything a HUD reads, so the runtime knows what it must supply. | Resolution — there is no game state yet. |
| **ENG-134** canvas editor | `EngineHudEditor.tsx` — a canvas at the reference resolution with the safe area drawn, widgets placed from **engine-resolved** rects, click-to-select. | The drag-to-move gesture. Selection and the numeric fields ship; dragging is the remaining half. |
| **ENG-135** Details panel | A form generated from `widget_schema` — text fields, number fields, enum dropdowns, bool selects, plus anchor/offset/size, style (bg, fg, fill, radius, opacity, font size, align), bindings and the click action. Committed as **one** undo step. | — |
| **ENG-136** HUD Outliner | The widget tree with depth indentation, per-widget visibility toggle and remove. | Drag-to-reparent (the `reparent_widget` action exists and is tested; the gesture is not wired). |
| **ENG-137** AI HUD actions | Thirteen `HudAction` kinds through `HudSessions`, the same path the Details panel uses — so a HUD the AI writes and a HUD a person edits go through one write path with one undo stack. Undo is a snapshot stack: a HUD is kilobytes, and snapshots cannot desynchronise from the document the way thirteen hand-written inverses could. | — |
| **ENG-138** runtime HUD | `engine_play_world` returns the HUD document **and** its widgets pre-resolved; the viewport draws them as a scaled overlay, reading the live session so unsaved widget edits appear the moment you press Play. Phase 6 completed it: bindings read live runtime variables, buttons fire into the runtime, and `hud_set` / `hud_show` let a script drive a widget directly. | — |
| **ENG-139** migration | The scaffold writes `assets/ui/hud_main.hud.json`; the manifest's `hud_scene` and Main's `settings.hud` point at it; `compose_play` no longer merges HUD entities into the 3D world (there is no `hud` layer tag any more). | An upgrader for projects that already have a `hud.bscn.json` — the old file is simply left alone rather than silently rewritten. |

---

### Phase 4 — Unreal-grade editor usability  ·  `ENG-140…152`

*"Make it look and work like Unreal Engine." Everything here is UX; none of it may add
business logic to the webview.*

- [ ] **ENG-140** RETIRED AS A DEFAULT-SHELL REQUIREMENT by ADR-0034 (advanced docking remains optional) — Docking system: panels are dockable / tabbed / floating / resizable,
      with saved layouts and presets (**Default**, **Level Design**, **Materials**,
      **HUD**, **Play**). Layout persists per project.
- [~] **ENG-141** (entity tree ships; organiser folders remain) — World Outliner, properly: real tree with expand/collapse, drag to
      reparent, **folders**, multi-select (Ctrl / Shift), per-row visibility + lock, type
      icons, column headers, filter chips (type, tag, component, "AI-generated"), and a
      search box that matches name/tag/component.
- [x] **ENG-142** (codex, 2026-09-01) — Details panel generated from the schema registry: category accordions
      (Transform, Rendering, Physics, Audio, Gameplay, Scripting), per-`FieldKind` editors
      (drag-number with range clamp, vector rows, colour picker, enum dropdown, asset picker
      with a searchable browser, boolean toggle, JSON fallback), reset-to-default arrows,
      multi-edit across a selection, a property search box, and **Add Component** driven by
      `schema::registry()`. Editing a shared component field with multiple entities selected
      emits one atomic batch, one journal row and one undo step.
- [~] **ENG-143** (claude, 2026-09-01 — listing + asset picker ship; tiles and thumbnails do not) — Content Browser: tile grid with thumbnails, folder tree + breadcrumb,
      type filters, search, right-click menu (Rename, Duplicate, Delete, Show in Explorer,
      Reimport, Replace Object, Create Material Instance), drag into the viewport to spawn,
      drag onto a mesh to assign. *Feeds `ENG-031` / `ENG-034`.*
- [x] **ENG-144** (codex, 2026-09-01) — Viewport toolbar in UE5 order: view mode (Perspective / Top / Bottom /
      Front / Back / Left / Right), shading (Lit / Unlit / Wireframe / Detail Lighting /
      Lighting Only / Collision), **Show** flags menu (grid, gizmos, icons, bounds, colliders,
      nav, HUD-in-viewport), camera speed, FOV, screen percentage, maximise.
      All seven camera directions and six shading modes are selectable; Collision shares
      the Play collider resolver, screen percentage changes the render pixel ratio, and
      maximise hides the auxiliary panels without changing authored state.
- [~] **ENG-145** (claude, 2026-09-01 — multi-select and focus ship; marquee, vertex snap and Alt-drag do not) — Selection tools: marquee/box select, Ctrl-click add/remove, `F` frame
      selected, `Shift+F` orbit-lock, isolate selection, vertex snap, **Alt-drag duplicate**,
      surface snapping (`End`). *Completes the vertex-snap / Alt-drag remainder of ENG-023c.*
- [x] **ENG-146** — Fix F9: transform hierarchy accumulates parent → child in the viewport,
      gizmos honour parent space, and reparenting preserves world transform by default
      (`Alt`-drop keeps local).
- [x] **ENG-147** (codex, 2026-09-01) — Command palette (`Ctrl+P` files, `Ctrl+Shift+P` commands) over every
      engine command, every scene, every asset — the same command list the AI uses.
      `Ctrl+P` searches the engine's scene catalogue and indexed assets; choosing a scene
      opens it and choosing an asset applies it through the normal transaction path.
- [~] **ENG-148** (2026-09-01 — the full transport ships; Selected Viewport/New Window/Simulate semantics remain) — Transport bar: **Play / Pause / Step / Stop / Eject**, a play-mode
      selector (Selected Viewport, New Window, Simulate), and a live stats readout
      (fps, ms, entities, draw calls).
- [~] **ENG-149** (codex, 2026-09-01 — the shared typed ring, filters, AI `get_console` and exact source navigation ship; restart persistence remains) — Output Log panel: engine/script/AI channels, level filter, search, click
      a stack line to open the file — and the same buffer feeds the AI's `get_console`.
- [x] **ENG-150** — Toast / notification lane for engine facts: "Agent moved 3 actors",
      "Material created", "Scene saved", each with an **Undo** affordance.
- [~] **ENG-151** (claude, 2026-09-01 — the UE5 keymap ships; rebinding does not) — Keyboard map matching UE5 (`Q W E R`, `X` space toggle, `Ctrl+Z/Y`,
      `Ctrl+D`, `Del`, `F`, `G` game view, `Ctrl+S`, `Alt+P` play, `Esc` stop) plus a
      rebindable keymap in Settings.
- [x] **ENG-152** (codex, 2026-09-01 — axe state matrix and source guards pass) — INV-075 pass on every engine panel: loading / empty / error / populated
      states, full keyboard reachability, AA contrast, focus rings, screen-reader labels.

**Acceptance:** a user who knows UE5 can open a Bhippi game, find the Outliner, Details,
Content Browser and viewport controls where they expect them, dock them how they like, and
edit a level without reading documentation.

*Core workflow met; full acceptance remains open.* The fixed layout supports the ordinary
Outliner → Details → viewport → Content Browser loop, including schema-safe multi-edit,
file quick-open and the measured accessibility matrix. Saved docking presets, organiser
folders, complete selection/snap tools, real asset thumbnails and key rebinding remain.

---

### Phase 4 — what shipped, precisely

| Ticket | Shipped | Deliberately not |
|---|---|---|
| **ENG-141** Outliner | A real **tree** with expand/collapse, drag-to-reparent (drop on empty space un-parents), multi-select via Ctrl/Shift, per-row visibility and lock writing the `Visibility` component through the normal action path, type icons, a search across name/tag/component, and filter chips including **AI-made** — which works because ENG-127 stamps provenance on every spawn. Filtering keeps ancestors visible so context is not lost. | Named folders. The tree is the entity hierarchy; UE5's arbitrary organiser folders are a separate concept the scene format has no field for. |
| **ENG-142** Details | Generated entirely from `bhippi-engine`'s registry over `engine_component_schema` — accordions by category, a control per `FieldKind`, schema-owned defaults and Reset, property search, Add/Remove Component, plus name and tags. Multi-selection computes common/mixed/unavailable state; shared writes commit atomically through the batch path, while unavailable fields are disabled instead of partially applied. | — |
| **ENG-143** Content Browser | Grid/list cards, folder tree, breadcrumb, search, asset glyphs, drag payloads and a context menu over real workspace files; `engine_list_assets` feeds schema-filtered Details pickers. | **Rendered thumbnails** (the current tile contains a type glyph), recursive/live folders, type filters, Rename/Duplicate/Delete/Reimport/Create Material Instance implementations and thumbnail invalidation. |
| **ENG-144** viewport toolbar | Perspective plus all six orthographic directions; Lit, Unlit, Wireframe, Detail Lighting, Lighting Only and Collision modes; truthful screen-percentage rendering and stats; maximise/restore; and editor-only Show flags that never author scene data. | — |
| **ENG-145** selection | Multi-select (Ctrl/Shift in the Outliner, tracked as a list and pushed to engine selection state so the agent sees it), double-click to focus. | Marquee/box select, vertex snap, Alt-drag duplicate, surface snapping. |
| **ENG-146** transform hierarchy | **The F9 fix.** Objects are now parented to each other in the viewport, so a `Transform` is local to its parent and moving a parent moves its children. This was a one-line comment admitting the hierarchy was "logical, not transform-accumulated" — and it is why prefabs, rigs and grouped level pieces could not work. Gizmo drags write the local transform, so a dragged child takes its own children with it. | — |
| **ENG-147** command palette | `Ctrl+Shift+P` over file, edit, add, weather, play, view and project commands, fed by the same handlers the toolbar calls. `Ctrl+P` covers scenes, HUD, material/script/assets and project-scoped stable recents; basename results retain full-path disambiguation, and missing entries are evicted after indexing. | — |
| **ENG-148** transport | Play/Pause/Resume, Step, Stop, Restart, Eject/Possess, time scale, Game View, pause-on-script-error and live fps/frame-ms/entity/contact/draw/script stats. | The play-mode selector (Selected Viewport / New Window / Simulate). New Window needs an explicit window lifecycle contract; it may not be a label that still runs in-pane. |
| **ENG-149** Output Log | A bounded typed Rust console shared by the panel and AI `get_console`, with engine/script/AI source, level/text filters, timestamps and exact file/line navigation. Runtime script faults enter the same source-aware path, and missing/escaping paths fail safely. | Persist the bounded console across process restart. |
| **ENG-150** toasts | Every applied change raises a toast naming the actor — **Agent** or **You** — with a one-click **Undo**. An agent edit arriving by event toasts too, so a change made while you were looking elsewhere is not silent. | — |
| **ENG-151** keymap | `Alt+P` play and `Ctrl+Shift+P` palette join the existing UE5 set (Q/W/E/R, X, Ctrl+Z/Y, Ctrl+D, Del, F, Ctrl+S). | A rebindable keymap in Settings. |
| **ENG-152** a11y | Engine panels carry semantic labels/states, real focusable controls and keyboard routes; the automated axe matrix covers 32 loading/empty/error/populated states with zero serious or critical findings, with source guards for zoom and reduced-motion behaviour. | — |

**ENG-140 (docking) — not started, deliberately.** A dockable/tabbed/floating layout system
with saved presets is a week of UI work whose benefit is arrangement, not capability. Every
other Phase 4 ticket makes something *possible that was not*; this one makes something
*movable*. It is the right thing to cut when the renderer still cannot draw a material
(F8, Phase 5). Left `[ ]` rather than half-built.

---

### Phase 5 — Rendering truth  ·  `ENG-160…168`

*Closes F8 and F9. Until the viewport shows the real scene, neither the user nor the AI can
verify anything.*

- [x] **ENG-160** — GLTF/GLB loading in the viewport with a shared cache and instancing;
      `MeshRenderer.mesh` resolves through the `AssetIndex`, not by string sniffing.
- [x] **ENG-161** — Primitive meshes become **real assets** (built-in cube/sphere/plane/
      capsule/cylinder/cone with stable ids), so `mesh: ""` and `mesh: "cube"` stop being
      two different conventions.
- [x] **ENG-162** — PBR material application: `MaterialOverride` and `.mat.json` maps
      (albedo / normal / roughness / metallic / ao / emissive) actually bind to the Three.js
      material, with correct colour spaces and tiling.
- [x] **ENG-163** — Lighting parity: directional / point / spot honour colour, intensity,
      range and cone angle from the `Light` component; shadow settings honoured.
- [x] **ENG-164** — Sky + environment: skybox asset, IBL, and the eight weather presets
      driving sky colour, fog, sun intensity and precipitation particles from data, not from
      a hard-coded TS table.
- [x] **ENG-165** (codex, 2026-09-01) — Editor visuals: infinite grid with adaptive spacing, billboard icons for
      lights/cameras/spawns, selection outline, wireframe overlay, collider debug draw,
      bounds display.
      Collider wireframes consume Play's exact shape resolver (box, sphere, capsule and
      sampled heightfield), inherit hierarchy/rotation, distinguish sensors, and render
      unknown shapes magenta while recording one typed physics error. Bounds update live.
- [x] **ENG-166** (codex, 2026-09-01) — Camera preview PiP when a `Camera` entity is selected (UE5 behaviour).
      The preview renders through the selected entity's authored Camera component in a
      bounded 16:9 scissor view and does not move the editor camera or expose the camera
      model inside its own picture. Resize/minimise/close controls, a no-active-camera state,
      hidden-render suppression, transform/FOV/aspect tests and a single-cache assertion ship.
- [~] **ENG-167** (claude, 2026-09-01 — the engine-side budget is measured in CI; frame rate is not) — Performance budget INV-077: ≥55 fps with 1 000 entities — instancing,
      frustum culling, LOD, throttled RAF when the pane is hidden, and a stats harness that
      **fails CI** below budget.
- [x] **ENG-168** — **DECIDED: ADR-0028.** — Decide the Bevy question. `ENG-010`'s embedded child-process viewport is
      still unbuilt while the Three.js viewport carries the product. Either build the Bevy
      viewport behind the JSON-RPC protocol that already exists in
      `bhippi-engine-viewport/src/protocol.rs`, **or** amend ADR-0020 to make the webview
      viewport the shipping renderer with its own budget. Do not leave this ambiguous — it
      decides where Phase 6's physics runs. *Blocks nothing else in this plan if the ADR is
      amended.*

**Acceptance:** a `.glb` with textures dropped into the Content Browser and dragged into the
viewport renders with its real geometry and materials; moving a parent moves its children;
1 000 entities hold ≥55 fps.

*Met, except the frame-rate number itself.* GLTF loads and draws with its real geometry;
`.mat.json` materials apply with all six PBR maps in the correct colour spaces; moving a
parent moves its children (ENG-146, Phase 4). The 55 fps figure is **not** asserted — it
needs a GPU and a browser, so `crates/bhippi-engine/tests/perf_budget.rs` measures the
engine-side half instead and says so.

---

### Phase 5 — what shipped, precisely

**ENG-168 is decided: [ADR-0028](adr/0028-webview-viewport-renderer.md).** The webview is the
shipping renderer; ADR-0020's Bevy child-process model is **withdrawn, not deferred**. The
13-line stub, the `bevy` feature, the dependency and the binary target are gone —
`protocol.rs` stays as a documented, unused design and the ADR names the conditions under
which it comes back. INV-072 and INV-078 are retired, INV-073 is narrowed and made precise
about what the webview may compute, INV-077 is kept and re-targeted. ENG-010…013 close as
**withdrawn**. This had been "next" in three consecutive session logs and was blocking
Phase 6.

| Ticket | Shipped | Deliberately not |
|---|---|---|
| **ENG-160** GLTF | `renderResources.ts` loads GLB/GLTF through a shared cache, normalises an imported model to unit scale so it does not arrive a hundred metres tall, and clones per instance so geometry is shared. A file that will not load is treated as missing, not as an empty success. | Draco/Meshopt compression. |
| **ENG-161** built-in meshes | `bhippi-engine::mesh` — eight primitives addressed as `builtin:<name>`. `MeshRenderer.mesh` now has **exactly three** legal forms: empty (unset), `builtin:…`, `asset:<ulid>`. The bare `"cube"` the old TypeScript wrote is rejected with a hint listing the real references. The scaffold and the spawn palette write `builtin:` refs. | — |
| **ENG-162** PBR materials | `engine_render_manifest` resolves every material the open scene references: parses the `.mat.json`, resolves `asset:` textures to files, fills defaults, and hands the viewport a flat description. `renderResources.ts` builds one shared `MeshStandardMaterial` per key with all six maps — **albedo and emissive in sRGB, normal/roughness/metallic/AO linear**, because a data map decoded as colour is subtly wrong lighting everywhere. Tiling and offset applied per map. | A material *editor*; the Details panel edits `MaterialOverride`, and `.mat.json` files are edited as files. |
| **ENG-163** lighting | Directional, point and **spot** honour colour, intensity, range and `outer_angle` from the component. `outer_angle` has been in the registry since it was written and the viewport drew every non-directional light as a bare point. Spots get a target in the graph, or Three aims them at the world origin. | Per-light shadow tuning beyond a 1024 map. |
| **ENG-164** sky and weather | The preset drives sky colour, fog range **and now the lights** — ambient colour and intensity, key-light intensity and tint — so picking "storm" darkens the scene rather than only repainting the backdrop. Numbers come from `bhippi-engine::weather`; the viewport keeps no copy. | Skybox textures and IBL. |
| **ENG-165** editor visuals | Named grid, helper icons, selection outline, wireframe overlay, bounds and collider debug drawing. Collider shapes use Play's resolved box/sphere/capsule/sampled-heightfield data with hierarchy/rotation and sensor distinction; unresolved shapes render magenta and emit a typed physics error. Show toggles remain editor-only. | — |
| **ENG-167** perf budget | `perf_budget.rs` runs in CI: a 1 000-entity scene dumps, validates and walks inside 250 ms; one edit on it stays inside INV-079's 50 ms; play composition of two 500-entity scenes stays linear; and **a thousand identical props collapse to one mesh and one material** — the property the renderer's cache depends on, which nothing else would notice breaking. | The frame rate itself. Asserting fps needs a GPU and a browser; claiming INV-077 without measuring it would be the kind of tick this document exists to prevent. |

**ENG-166 (camera preview PiP) — complete.** A second bounded 16:9 scissored render shows
the selected `Camera` entity's authored transform/FOV without moving the editor camera or
duplicating the asset cache. Resize/minimise/close, hidden-render suppression, a labelled
no-camera state and automated camera/resource fixtures ship.

#### Closure backlog for Phases 0–5

These are not vague polish notes. Each row is the smallest shippable remainder that closes
the ticket above. Work in this order: **truth/events → provider bridge → authoring gestures →
editor arrangement/tools → measured quality**. Do not let docking or thumbnails delay a
correct conflict/observation path.

| Ticket | Exact remaining deliverable | Primary seam | Evidence that permits `[x]` |
|---|---|---|---|
| ENG-107 | Apply `touched` entity/component patches to existing viewport objects; reserve a full rebuild for scene-open/schema-reset. Coalesce rapid transform events without dropping the final value. | `EngineSceneChanged` / `EngineTransactionApplied`, `EngineView.tsx`, `EngineViewport.tsx` | integration timestamps transaction commit → Outliner/Details/viewport projection p95 ≤50 ms at 1k entities; test proves untouched GLTF/material instances keep identity |
| ENG-108 | Three-way conflict view: saved base vs. dirty session vs. disk, grouped by entity/component/field; **Keep mine**, **Take disk**, and per-hunk apply all create normal journalled transactions. | `engine/session.rs` + a pure Rust scene diff; conflict panel in `EngineView.tsx` | fixture covers independent merge, same-field conflict, disk delete and malformed disk; no choice silently overwrites either input |
| ENG-110 | Finish only verbs whose domain exists: project scene/level manifest changes; editor selection/camera/view/gizmo; play controls; remaining content/HUD/script verbs. Each group is one enum/schema addition with capability mapping and inverse rules. | `bhippi-engine::action`, `engine/content`, `hud_action`, app bridge | golden schema inventory asserts every prompt verb maps to one implemented action and every action maps to a capability; no prompt-only verb |
| ENG-114 | Write an ADR extending `CompletionRequest` with provider-neutral tool definitions and `Delta` with tool-call start/arguments/end/result. Specify CLI-provider behaviour (native vendor loop vs. compatibility tags), ordering with text deltas, cancellation and usage accounting. Implement one HTTP adapter first; tags remain conformance fallback. | `bhippi-providers` public contract + `chat.rs` | provider contract suite replays split JSON arguments, parallel calls, cancellation, malformed args and fallback; native and tag paths yield the same `EngineActionBatch` |
| ENG-116 | Inline **Edit** for Ask-mode plan cards: edit label, remove/reorder actions, and schema-edit fields; revalidate and recompute capability verdict before approval. Never let edited JSON bypass typed decoding. | permission card + `EngineActionBatch` validator | UI test edits a destructive batch into a non-destructive one and vice versa; approval prompt/verdict changes correctly; rejected edit writes nothing |
| ENG-117 | Emit typed subsystem facts at the source: asset/material/shader/script created/compiled, game started/stopped, runtime fault, build completed. Feed a capped newest-first summary into the next model round. | `bhippi-types::EngineEvent`, existing ≤20/s bus | event conformance test asserts each subsystem emits success + failure once with ids/paths, and a burst stays within INV-076 without losing terminal facts |
| ENG-124 | ADR the mesh/image conversion stack and supported source matrix. Convert OBJ/FBX to canonical GLB with unit/axis/material report; normalise textures with declared colour space. Preserve original source + import recipe in sidecar for Reimport. | `engine/content.rs` or build/import module; no conversion in TS | frozen cube/rig/material fixtures compare bounds, handedness, UVs, material slots and hashes; unsupported feature fails by name; reimport is deterministic |
| ENG-134 | Pointer drag + eight resize handles use engine-resolved rects; snap to safe area, anchors, siblings and canvas centre; one gesture is one HUD undo step. Multi-select alignment/z-order must call HUD actions, not mutate TS state. | `EngineHudEditor.tsx` ↔ `HudSessions` interaction commands | pointer test covers each anchor quadrant, zoom scale, cancel/Esc and one-step undo; saved JSON reopens at exact rect |
| ENG-139 | Detect legacy HUD scene, show one previewable migration, write `hud_main.hud.json` only after approval, update manifest/Main in one recoverable operation, retain backup until Save. | Rust migration plan + content/session transaction | golden legacy fixtures migrate byte-identically; cancel changes nothing; second run is idempotent |
| ENG-140 | Dock model: split tree + tab stacks + floating windows, minimum sizes and missing-panel recovery. Persist a versioned layout per project; presets are data. Keyboard move/focus and reset-layout are first-class. | new Rust-validated `EngineLayout` persistence + thin React renderer | round-trip/default/upgrade/corrupt-layout tests; pointer + keyboard docking tests; every preset opens with all required panels reachable |
| ENG-141 | Add organiser folders as presentation metadata separate from entity parents, with rename/move/delete semantics that never alter transforms. | scene/editor metadata transacted in Rust | moving entities between folders leaves scene hierarchy/transforms byte-identical; deleting non-empty folder offers flatten, not entity delete |
| ENG-143 | Real thumbnails for meshes/materials/textures/scenes with cache key = asset hash + importer/renderer version; implement context actions through commands and gates. | background thumbnail service + Content Browser | four-state UI tests; rename preserves asset id/refs; delete lists blockers; reimport invalidates thumbnail; stale cache cannot display after hash change |
| ENG-145 | Marquee selection, Shift+F orbit lock, isolate, vertex snap, Alt-drag duplicate and End surface snap. Duplicates/transform results are computed/committed through engine actions. | raycast/gesture capture in viewport; transaction math in Rust | gesture tests across parented/scaled entities; one gesture = one undo; cancel leaves no duplicate; locked entities unchanged |
| ENG-148 | Decide and implement real semantics for Selected Viewport / New Window / Simulate. New Window must own lifecycle/input/focus/close; Simulate must not possess a game camera. | play controller + Tauri window seam | mode contract tests; closing New Window stops/discards runtime; all modes preserve authored bytes |
| ENG-149 | Persist the existing bounded typed console across restart per project without changing the panel/query filtering contract. | Rust console store persistence behind the existing DTO/API | restart/cap/redaction tests; restored records preserve source locations; panel and model query still return identical filtered records |
| ENG-151 | Versioned, conflict-checked keymap in Settings with Reset UE5 defaults; reserve OS/app shortcuts and show conflicts before Save. | Rust config validation + generated action catalog | round-trip, duplicate/reserved binding and upgrade tests; all engine commands remain keyboard reachable |
| ENG-167 | Add frustum culling/true instancing/LOD where measurements justify them, plus the browser GPU harness named in ENG-195. | `renderResources.ts`, viewport stats harness | reference run ≥55 fps/p95 ≤18.2 ms at 1k entities; hidden pane throttles; visual hash/pick identity stays correct under instancing |

**Closure rule:** a row may be split into smaller commits, but its parent ticket remains
`[~]` until the named evidence passes. If implementation chooses a materially different
seam, write/amend the ADR first and update this table in the same change.

---

### Phase 6 — Play mode that actually plays  ·  `ENG-170…180`

*Closes F5. The owner's line: "when user opens the main and presses run, the game should
start playing in this engine so they can test it."*

- [x] **ENG-170** — Play composition in **Rust**: `engine_start_play { scene, mode }` builds
      the runtime world — Main (persistent) + the chosen level + the HUD document — with
      stable id namespacing. Deletes TS `mergeScenes` (the current one produces invalid ids).
- [x] **ENG-171** — Snapshot / restore: entering play snapshots the authored world; **Stop**
      restores it exactly. Play must never write to the authored scene files (an invariant
      worth its own row in `06-INVARIANTS.md`).
- [x] **ENG-172** (claude, 2026-09-01) — Physics: gravity from the manifest,
      static/dynamic/kinematic rigid bodies, cuboid/sphere/capsule/mesh/heightfield
      colliders, sensors and collision events. *Feeds `ENG-053`.* Backend choice is gated on
      ENG-168.
- [x] **ENG-173** (claude, 2026-09-01) — Character controller: capsule, ground check, step
      height, slope limit, gravity, jump — driven by the `CharacterController` component
      fields, not by constants.
- [x] **ENG-174** — Camera possession: play uses the scene's `Camera` entity (or PlayerStart
      + follow camera), not the editor's fly camera; **Eject** returns to the editor camera
      while the sim keeps running.
- [x] **ENG-175** — Input manager: named actions and axes (`move_x`, `jump`, `fire`) mapped
      to keyboard / mouse / gamepad in `assets/input.json`, hand-editable and AI-editable —
      so `key_prompt` HUD widgets can resolve their glyphs.
- [x] **ENG-176** (claude, 2026-09-01 — **ADR-0030**) — Script runtime, Track B first
      (`EngineTrack::Scripted`): Rhai scripts with `on_start`, `on_update(dt)`,
      `on_collision(other)`, `on_trigger`, plus a narrow API surface (transform, find, spawn,
      destroy, set var, play sound, load level, HUD set). Errors surface as typed faults with
      file/line. *Feeds `ENG-051`.*
- [x] **ENG-177** (claude, 2026-09-01) — Runtime HUD (from ENG-138) wired to real state:
      health, score, timer, ammo, level name, and button actions firing into the runtime —
      plus `hud_set` / `hud_show` from scripts, which win over bindings for that frame.
- [x] **ENG-178** — Level travel: `load_level(name)` at runtime keeps Main and the HUD
      persistent, streams the new level in. *Completes `ENG-033`.*
- [x] **ENG-179** (claude, 2026-09-01) — Play diagnostics: fps / frame-ms / entity /
      contact / draw-call / script stats, a runtime error channel into the Output Log
      (script faults, script `log()`, triggers, sounds), and a **Break** toggle for
      pause-on-error.
- [x] **ENG-180** — Play controls: Pause, **Step one frame**, Stop, Restart, slow-motion,
      and a Game View (`G`) that hides editor gizmos.

**Acceptance:** double-click **main** → press **Play** → the game runs: the player moves with
input, collides with level geometry, the HUD shows live values, a button pauses, a trigger
loads level 2, and **Stop** returns the editor to exactly the authored state.

**2026-09-01 — Phase 6 complete.**

**Physics (ENG-172/173).** The conservative AABB solver is gone. `Collider.shape` is read for
real — cuboid, sphere, capsule and heightfield, with `mesh` documented as resolving against
the mesh's box rather than pretending to be exact — and every contact is a
sphere/capsule-against-**oriented**-box test that returns a normal and a depth. That is what
makes a rotated ramp a ramp: `max_slope` now decides whether a contact carries the controller
or it slides, and `step_height` climbs a ledge instead of walking into it. The character
capsule comes from `CharacterController`'s own `height`/`radius`, not from the transform's
scale.

**Scripts (ENG-176) — [ADR-0030](adr/0030-script-runtime-rust-compiler-webview-vm.md).** The
blocker was real and the ADR names it: Rhai is a Rust crate, and ADR-0028 put the runtime in
the webview. Ticking a box either way would have been a lie — running Rhai in Rust means an
IPC round trip per scripted entity per frame, and `eval` in the pane means no spans, no
sandbox and the language's semantics in TypeScript. The decision splits it: `bhippi-engine::script`
lexes, parses and compiles a **documented subset of Rhai** to bytecode with a per-instruction
line table, and `ui/src/engine/scriptVm.ts` is a ~400-line stack VM with no `eval`, no
`Function` and no host call it was not handed. Constructs outside the subset are rejected
**by name** (`for` says to use `while`, and why), a misspelled host call suggests the real
one by edit distance, and wrong arity is a compile error rather than a frame-1 surprise.
`while true {}` hits a 200 000-step budget and becomes a located red line instead of a frozen
pane. `create_script` compiles before it writes, so a script the AI got wrong comes back with
a line in the same turn.

The two languages are kept honest by a shared artefact: `ui/tests/fixtures/pickup.rhai` is
compiled by a Rust test that asserts the committed `pickup.program.json` is still what the
compiler emits, and the Node test executes that exact file. A bytecode change fails in Rust,
loudly, instead of silently producing a program the VM misreads. A second guard asserts
`prompts/chat-engine.md` lists every host function the compiler accepts — otherwise the model
writes scripts against a vocabulary that has moved.

**New: INV-082** — a gameplay script is compiled in Rust before it runs; the webview never
parses script source. Enforced by `tests/architecture.rs`, which greps the pane for `eval`,
`new Function` and friends.

**Deliberately not:** angular momentum, restitution and stacked rigid bodies (the solver is
kinematic and says so); exact mesh colliders; and Rhai's standard library beyond the 43
host functions listed in the prompt.

---

### Phase 7 — AI autonomy loop  ·  `ENG-185…192`

*The difference between "the AI can call the engine" and "the AI can build and verify a game."*

- [x] **ENG-185** (codex, 2026-09-01) — The loop driver (§80): understand → inspect project → inspect scene →
      plan → act → run → inspect result → check errors → fix → verify. The cap is
      `ENGINE_AUTONOMY_MAX_ROUNDS`; reaching it returns control with the last verified fact
      and unresolved fault, never a success claim. Every batch produces a visible **Engine
      plan** Activity Dock step before application.
- [x] **ENG-186** (codex, 2026-09-01) — Visual verification (§79):
      `<engine_query>{"kind":"screenshot","camera":"editor|game|entity:<id>",
      "annotate":true}</engine_query>` returns the exact viewport PNG for multimodal
      providers. Annotation may add entity names, selection, bounds and the active camera,
      but must not alter the live scene. Payload bytes, wait time and one-shot lifetime are
      bounded in Rust.
- [x] **ENG-187** (codex, 2026-09-01) — AI playtest (§94): start a disposable runtime, drive a
      bounded scripted input sequence using `KeyboardEvent.code`, sample transforms/runtime
      variables/events/errors at named checkpoints, stop, assert authored bytes unchanged,
      and return a structured report. A screenshot is a separate observation; a headless
      report must never pretend it saw visual composition.
- [x] **ENG-188** (codex, 2026-09-01) — Error recovery (§84): shader/script/asset/schema/runtime
      failures come back as typed errors with file, line/field, failing action index and
      hint. The AI patches, recompiles or re-applies, then repeats the exact failed check.
      Structural faults such as "no game manifest" terminate with a remedy rather than
      burning all six rounds.
- [x] **ENG-189** (claude, 2026-09-01) — AI action history + **Undo AI Change** as one
      operation over a whole batch, rendered from the journal (needs ENG-103).
- [x] **ENG-190** (claude, 2026-09-01) — Capability permissions (§88): per-project switches
      for what the agent may do — edit scenes, create assets, delete, import from disk, run
      play, write scripts, build. Default: edit + create, ask before delete/import/build.
- [x] **ENG-191** (codex, 2026-09-01) — Context manager (§86): the stable doctrine is
      versioned once; dynamic engine context is retrieval-shaped and capped to
      `ENGINE_CONTEXT_TOKEN_BUDGET` — selection + nearby entities, open-scene digest, dirty
      state, recent transactions and recent errors, never the whole scene. Overflow ends
      with an explicit instruction to use `engine_query`, and the measured engine-context
      category is compared with `docs/token-engine/baseline.md`.
- [x] **ENG-192** (claude, 2026-09-01) — Multi-agent safety (§87/§89): scene-level locking so
      two agents (or an agent and the user) cannot commit conflicting transactions; the loser
      gets a rebase prompt, never a silent overwrite.

**Acceptance:** given "build me a small warehouse level with a locked door and a key", the AI
plans, builds, plays, notices its own bug, fixes it, and reports — and one click undoes all
of it.

**2026-09-01 — Phase 7 is complete.** The exact-canvas camera bridge, bounded PNG/one-shot
contract, fixed-step authored-hash playtest, four repair fixtures, fixed dynamic-context
budget and offline warehouse repair transcript all pass. Repeated patches, structural
observation faults and the true six-round total stop with an unresolved remedy.

| Ticket | Shipped | Deliberately not |
|---|---|---|
| **ENG-189** Undo AI Change | The unit is the transaction, and a batch is already one transaction (ENG-111), so "undo everything the agent just did" is **one** row in the Output Log with one button. The inverse comes from the journal, not the in-memory undo stack, so it still works after a restart — and it is applied as a **new user transaction**, so reverting is itself undoable and appears in the history. A silent rewind that could not be taken back would be the more surprising behaviour. | Undoing a *range* of transactions in one press. |
| **ENG-190** capabilities | `bhippi-engine::capability` with seven capabilities and **three** states — allow / ask / deny. Three, because the useful question is not *may the agent delete things* but *may it delete things without showing me first*, and collapsing that into a boolean is what makes people switch the whole gate off. Stored in `[agent]` in `Bhippi.game.toml`, so it is per project, hand-editable, and reviewable in a diff; a typo there is **refused**, because a switch that silently means "default" is decorative. Enforced in `apply_batch_in_workspace` — the one choke point both agent paths use — and explicitly **not** applied to `EngineActor::User`. | Per-scene or per-folder scoping. |
| **ENG-192** multi-agent safety | A scene lease plus an optimistic revision check. The lease names the holder when a second agent tries; the revision check catches the case a lock alone misses — the user edits *while* an agent is thinking, and the agent's next batch is refused with "changed under you: you last saw revision 4, it is now at 6" rather than applied over the top. The lease is refreshed only by its own holder, which is what makes a user edit visible to the agent instead of being quietly absorbed. The user is never blocked. | Cross-process leases; the TTL (120 s) is the only recovery from a crashed agent. |

#### Phase 7 closure sequence — do not parallelise inside one scene

1. **ENG-186 capture contract first.** Finish camera selection (`editor`, possessed game
   camera, explicit Camera entity), reject a missing/inactive pane with a typed hint, and
   verify PNG signature/dimensions/byte cap before attaching it to a provider request.
2. **ENG-187 deterministic runner second.** Validate every input step in Rust before the
   webview sees it; cap steps, keys, key length and frames; use a fixed delta; always stop in
   `finally`; return `authored_hash_before == authored_hash_after` in the report.
3. **ENG-188 repair evidence third.** Freeze one bad script, one dangling asset, one invalid
   material and one runtime collision failure. Each fixture must prove: located fault → one
   corrected write → the same verifier goes green.
4. **ENG-191 measurement fourth.** Record an engine-context sample for empty, 50-entity and
   1 000-entity scenes. The 1 000-entity input must stay within the same 1 500-token dynamic
   budget; deep facts are retrieved, not injected.
5. **ENG-185 golden loop last.** The warehouse/key/locked-door fixture is the only legal
   phase tick. It must show the plan, apply one or more labelled batches, run the scripted
   playtest, consume at least one real failure, repair it, capture the final viewport and
   stop within the cap.

#### Phase 7 implementation contract

| Ticket | Authoritative seam | Required tests/evidence | Failure behaviour |
|---|---|---|---|
| ENG-185 | `chat.rs` engine loop + `engine/bridge.rs`; constants in `bhippi-types` | `engine_autonomy_golden.rs`: deterministic provider transcript; asserts visible plan steps, ≤ cap, final verifier success and no unverified success language | cap reached → final answer names unresolved fault + last successful check; lease conflict → re-query revision and re-plan |
| ENG-186 | `engine/observation.rs` ↔ typed Tauri events ↔ `EngineViewport.tsx` | Rust queue tests (timeout, late/duplicate response, oversize, invalid PNG); UI capture test (annotation on/off, exact dimensions); one multimodal-provider fixture proving image attachment | no pane/timeout/bad image → typed observation failure; never fall back to a desktop screenshot silently |
| ENG-187 | Rust request validation ↔ `runScriptedPlaytest` in `playRuntime.ts` | deterministic replay test; invalid-key/zero-frame/over-budget tests; start/stop cleanup test; authored-byte-identity assertion; fixture reporting a fall-through coordinate | malformed plan rejected before Play; runtime exception stops/discards the clone and returns partial samples + fault |
| ENG-188 | `EngineActionOutcome`, script compiler faults, content gates, Output Log | four repair fixtures named above; each asserts the corrected artefact and re-run result, not merely a second model response | non-repairable fault exits early with actionable user remedy; repeated identical patch is detected and stops |
| ENG-191 | `engine_context()` + Token Engine `ContextSampleStore` | Unicode-safe cap unit test; 3-size baseline report committed under `docs/token-engine/`; assertion that dynamic facts never exceed budget + suffix allowance | truncation occurs only at a UTF-8 boundary and always advertises the retrieval tool |

**Phase 7 acceptance evidence:** `engine_autonomy_golden::warehouse_key_door_repairs_and_verifies`
passes with the network disabled against a deterministic provider; the transcript includes a
viewport image attachment and scripted playtest report; the journal can undo the entire AI
build in labelled batches; Stop leaves authored scene/HUD/script bytes identical except for
the intentional authoring transactions.

---

### Phase 8 — Hardening  ·  `ENG-195…199`

- [ ] **ENG-195** — Performance gates, split honestly by environment:
      - **CI/headless:** INV-079 transaction apply **and event projection** ≤50 ms at 1 000
        entities; render-manifest generation, scene validation, play composition and context
        retrieval budgets; regression results stored as machine-readable artefacts.
      - **GPU/browser lane:** INV-077 ≥55 fps at 1 000 visible entities at the documented
        viewport size, warm asset cache and reference machine; also record p95 frame time,
        draw calls and main-thread long tasks.
      - **Retired:** INV-078 cold child-process attach. ADR-0028 removed that process; do not
        recreate or measure it. Replace it with first-use webview readiness only if a new
        invariant and measurement protocol are accepted first.
- [x] **ENG-196** — Crash recovery + autosave QA: kill the app mid-edit, reopen, recover.
- [x] **ENG-197** (codex, 2026-09-01) — INV-075 accessibility gate across Engine shell, Outliner, Details,
      Content Browser, HUD editor, Output Log, permission plan card and play controls:
      keyboard-only task path; visible focus; correct names/roles/states; no colour-only
      actor/error/lock meaning; AA contrast; reduced motion; focus restoration after menus,
      dialogs and Play; axe has zero serious/critical findings in all four states.
- [x] **ENG-198** (codex, 2026-09-01) — Documentation closure:
      - `04-PAGES.md`: loading/empty/error/populated states, dock/preset behaviour, HUD mode,
        play mode, AI plan/observation surfaces and complete keyboard map.
      - `02-MODULE-CONTRACTS.md`: session/event/observation/playtest APIs, ownership split
        between Rust document truth and webview picture/runtime, capability and lease rules.
      - `06-INVARIANTS.md`: keep INV-072/078 retired; retain INV-081/082; add explicit format
        validation and observation/playtest boundedness rows only if not already covered by
        an existing invariant.
      - ADRs: ADR-0028 (webview renderer) and ADR-0030 (script compiler/VM) already exist;
        write one accepted ADR covering the `bhippi-hud@1`, `bhippi-material@1`,
        `bhippi-shader@1` and `bhippi-prefab@1` compatibility/versioning policy rather than
        four repetitive ADRs.
      - Reconcile `08-BUILD-ORDER.md`, `PROGRESS.md`, this status line and every checkbox
        against tests in the same change. No document may still promise Bevy child attach.
- [ ] **ENG-199** — Golden end-to-end release test, with two lanes:
      - **Network-free deterministic lane:** scaffold → deterministic model builds level,
        material, script and HUD → simulated user edits scene + HUD through public commands →
        playtest + screenshot → stop → Debug/Release preflight → journal/undo/recovery checks.
      - **Host toolchain lane:** build Windows and Web artefacts, launch each smoke target,
        verify input + HUD + level travel, then assert `engine_builds` ledger hash, size,
        duration, target and status. Host-unavailable targets skip with a named doctor reason;
        a requested available target may not silently skip.

#### Phase 8 required fixtures

| Fixture | Purpose | Must assert |
|---|---|---|
| `tests/fixtures/engine/warehouse_game/` | canonical authored + AI-edited game | deterministic file tree and hashes; one intentional repair in the autonomy transcript |
| `tests/fixtures/engine/a11y_states.json` | all four UI states per panel | keyboard reachability and axe zero serious/critical |
| `tests/fixtures/engine/perf_1000.bscn.json` | stable 1k-entity workload | fixed entity/component/material mix; no generated-at-test randomness |
| `tests/fixtures/engine/unlicensed_release/` | INV-074 blocker | Release fails and names the exact asset; Debug exposes a warning without mutating metadata |
| `tests/fixtures/engine/crash_recovery/` | INV-071/ENG-196 | recovered dirty state equals last journalled revision; authored file remains untouched until Save |

**Acceptance:** every active budget has a reproducible protocol and a passing result; axe has
zero serious/critical findings; deterministic E2E passes offline; Windows and Web artefacts
run on their available host lanes; ledger rows match their files; all authoritative docs
describe the implementation that produced those results.

---

### The mandatory `/gamedebug` architecture

`/gamedebug` is not a prompt shortcut and the model does not decide what it checks. It is a
versioned, engine-owned pipeline that runs the same ordered stages for every Bhippi game.
The command may add game-specific scenarios, but it may never skip a mandatory stage.

```
/gamedebug [quick|full|release] [--fix]
       │
       ▼
GameDebugRunner (Rust; fixed stage registry, versioned as bhippi-game-debug@1)
       │
       ├─ 01 discover .... canonical root, manifest, default scene, HUD, levels, targets
       ├─ 02 validate .... formats, schemas, refs, licences, hierarchy, input and capabilities
       ├─ 03 compile ..... scripts, shaders and target-independent build preparation
       ├─ 04 sandbox ..... fresh runtime clone, denied-by-default broker, resource budgets
       ├─ 05 exercise .... deterministic smoke route + project GameTestPlan scenarios
       ├─ 06 inspect ..... faults, logs, events, collisions, progress, soft-lock/liveness probes
       ├─ 07 observe ..... checkpoint state and bounded screenshots; no visual claim headlessly
       ├─ 08 score ....... versioned quality rubric with evidence for every score
       └─ 09 report ...... atomic JSON + Markdown; immutable run id and authored-tree hash
```

The canonical machine report is `.bhippi/reports/game-debug/<run-id>.json`; the human report
is the same basename with `.md`; `latest.json` is an atomically replaced copy/pointer. Reports
are runtime artefacts and stay out of source control. The JSON envelope is:

```json
{
  "format": "bhippi-game-debug@1",
  "pipeline_version": 1,
  "run_id": "ulid",
  "mode": "quick|full|release",
  "project": { "root": "…", "authored_hash": "sha256:…", "manifest": "Bhippi.game.toml" },
  "environment": { "app_commit": "…", "os": "…", "renderer": "headless|webview", "gpu": null },
  "summary": { "status": "pass|fail|incomplete", "blockers": 0, "errors": 0, "warnings": 0 },
  "stages": [],
  "findings": [],
  "quality": { "rubric_version": 1, "score": null, "dimensions": [] },
  "repair_plan": [],
  "artifacts": [],
  "authored_hash_after": "sha256:…"
}
```

Every finding has a stable code, severity, stage, file/entity/widget/script address, observed
fact, expected fact, evidence references, reproducible steps, and a bounded suggested repair.
The AI receives the JSON report as retrieval context and must cite finding codes in its plan.
Default `/gamedebug` is read-only. `--fix` still uses the normal capability/approval gate,
applies labelled `EngineActionBatch` transactions, re-runs the failing stage and appends a
repair attempt; it never edits files through a separate debugger write path. A report may be
`pass` only when every mandatory stage completed and the authored hash stayed unchanged.

---

### Phase 9 — AI game-generation quality foundations  ·  `ENG-200…209`

*First quality phase: replace “looks good to the model” with measured, reproducible evidence.*

- [x] **ENG-200** — Write ADR-0032 before code: define `bhippi-game-debug@1`,
      `bhippi-game-test-plan@1`, rubric versioning, report retention, headless-vs-visual truth,
      and the fixed stage registry. Unknown versions block.
- [~] **ENG-201** — Add pure `bhippi-engine::game_debug` domain types: mode, stage id/status,
      finding/evidence addresses, report, quality dimension and repair step. JSON round-trip,
      sorted output and schema fixtures are required.
- [ ] **ENG-202** — Add `GameTestPlan`: deterministic named scenarios with initial level,
      seed, input steps, checkpoints and assertions over variables/events/transforms/HUD/level
      travel. The engine supplies a mandatory smoke scenario when the project has none.
- [ ] **ENG-203** — Implement rubric v1 with evidence-backed dimensions: bootability,
      goal clarity, control correctness, progression/finishability, failure/recovery, runtime
      stability, visual legibility, HUD feedback, content coherence and performance. A missing
      observation yields `not_measured`, never a guessed score.
- [ ] **ENG-204** — Canonical quality corpus: warehouse key/door, platformer checkpoint,
      top-down collection loop, HUD-driven puzzle and deliberately broken variants. Freeze
      prompts, seeds, provider transcript, file hashes and expected finding codes.
- [ ] **ENG-205** — Static and semantic inspectors: unreachable level refs, missing spawn or
      possessed camera, input action with no consumer, objective with no completion event,
      impossible key/door dependency, orphan HUD binding, unbounded spawn loop and silent
      runtime fault. These are engine analyses, not string advice in the prompt.
- [~] **ENG-206** — Implement stages 01–03 of `GameDebugRunner` by composing existing manifest,
      content, build, script and shader gates without duplicating them.
- [~] **ENG-207** — Atomic report store with run ULID, authored hash, stage timings, retained
      partial reports after failure, `latest` update and bounded artefact retention.
- [~] **ENG-208** — Local `/gamedebug quick|full|release` interception and composer discovery.
      It works offline and without a provider, renders the Markdown report, and links exact
      files/entities/findings. Bad arguments return usage without starting a run.
- [ ] **ENG-209** — Quality baseline command/CI artefact. Record the corpus result by rubric
      version and fail on new blockers, newly unmeasured required dimensions, or a statistically
      meaningful regression—not on harmless report ordering.

**Acceptance:** `/gamedebug full` on every canonical corpus game produces byte-stable report
semantics, finds every seeded defect by its stable code, makes no unsupported visual claim,
and leaves the complete authored tree byte-identical.

**First slice shipped:** ADR-0032; the fixed nine-stage Rust graph; `quick` manifest, scene,
asset-reference/licence and `.rhai` compilation checks; authored-tree before/after hashes;
stable evidence/reproduction/repair findings; command parsing/composer discovery; immutable
ULID JSON + Markdown reports and a latest pointer. ENG-201 remains partial until the full
quality/repair types and schema golden land; ENG-206 until shader/build composition lands;
ENG-207 until timings, atomic cross-platform latest replacement and retention land; ENG-208
until runtime stages and exact clickable addresses land. `full`/`release` honestly return
`incomplete` with selected runtime stages `unsupported`.

---

### Phase 10 — AI game-generation quality improvement loop  ·  `ENG-210…219`

*Second quality phase: make the report actionable enough for the AI to improve the game safely.*

- [ ] **ENG-210** — The chat engine retrieves the latest compatible report and injects only
      summary + selected findings; the model must query deeper evidence. Stale authored hashes
      are labelled stale and cannot justify a repair.
- [ ] **ENG-211** — `/gamedebug --fix` creates a visible repair plan grouped by finding code,
      asks according to project capabilities, commits one labelled batch per coherent repair,
      and re-runs the exact failed stage. No direct debugger writes.
- [ ] **ENG-212** — Repair convergence guard: cap attempts per finding, detect identical patches
      and oscillation, preserve the best verified state, and return the unresolved evidence
      instead of spending turns indefinitely.
- [ ] **ENG-213** — Finishability testing: deterministic state-space/path probes for authored
      objectives plus scenario replay. Report `proven`, `failed` or `unknown_with_bound`; never
      claim a game is beatable from a screenshot or elapsed play time.
- [ ] **ENG-214** — Visual quality lane: fixed cameras/resolutions/theme, screenshots for main
      gameplay/HUD/failure/win states, clipping/contrast/occlusion/empty-frame checks and optional
      multimodal critique stored separately from deterministic scores.
- [ ] **ENG-215** — Control-feel and pacing probes: input-to-motion latency, stuck-input recovery,
      camera target visibility, checkpoint spacing, no-progress windows and soft-lock detection.
      Thresholds live in the versioned test plan/rubric, not provider prose.
- [ ] **ENG-216** — Generation diversity checks detect duplicated layouts, repeated entity names,
      degenerate placement and asset overuse while explicitly avoiding a single “house style”
      score that punishes valid genres.
- [ ] **ENG-217** — Before/after comparison: each repair report names changed transactions,
      resolved/new/regressed findings and dimension deltas; a fix that introduces a blocker rolls
      back as a normal journalled transaction.
- [ ] **ENG-218** — Quality dashboard in the Engine Activity/Output surface: current run, stages,
      findings by severity, scenario evidence, score confidence, artefacts, repair history and
      “Undo AI Change”. It renders report truth and owns no scoring logic.
- [ ] **ENG-219** — Live-provider evaluation lane across supported model families, separated from
      deterministic CI. Record prompt/model/version/seed/cost/attempts; compare against the frozen
      corpus and require human review for subjective visual/taste changes.

**Acceptance:** starting from each broken corpus fixture, `/gamedebug full --fix` consumes the
machine report, repairs every repairable seeded defect within the attempt cap, reruns the same
checks, introduces no new blocker, shows a traceable before/after report, and can undo every AI
repair without affecting prior user work.

---

### Phase 11 — Runtime sandbox foundation  ·  `ENG-220…229`

*First sandbox phase: make gameplay execution isolated, capability-brokered and disposable.*

- [ ] **ENG-220** — Write ADR-0033 before code: retain ADR-0028's webview renderer, but move the
      bytecode VM and gameplay simulation into a dedicated module worker. State clearly that the
      worker is defence in depth, while safety comes from compiled bytecode + brokered host calls.
- [ ] **ENG-221** — Versioned `bhippi-runtime-protocol@1` messages with monotonic sequence numbers,
      session nonce, payload caps and exhaustive request/result/fault variants. Unknown or out-of-
      order messages terminate the disposable session.
- [ ] **ENG-222** — A fresh worker per play/debug run; it receives only the runtime snapshot,
      compiled bytecode and declared capabilities. No authored path, provider token, environment,
      DOM handle, arbitrary module URL or raw filesystem/network primitive crosses the boundary.
- [ ] **ENG-223** — Deny-by-default host capability broker for entity reads/writes, input, HUD,
      level travel, audio and timers. Every host function is mapped to exactly one capability and
      validated in Rust before the run plus at the broker boundary during the run.
- [ ] **ENG-224** — Enforce budgets: instructions/tick and total, call depth, entities spawned,
      events/log bytes, message size/rate, timers, heap-estimate and wall-clock watchdog. Budget
      exhaustion returns a typed fault and destroys the runtime clone.
- [ ] **ENG-225** — Deterministic clock and seeded RNG; no wall clock or ambient randomness in the
      script ABI. Replaying the same snapshot/seed/input produces the same checkpoint hashes.
- [ ] **ENG-226** — Runtime CSP and packaging: worker source is application-owned and hash-pinned;
      no `eval`, `Function`, dynamic import, `importScripts`, inline worker source or remote worker.
      Architecture tests block their reintroduction.
- [ ] **ENG-227** — Fault containment: panic, infinite loop, malformed bytecode, invalid host call,
      worker exit and timeout all stop Play, preserve authored files, capture partial telemetry and
      allow a clean restart with a new nonce.
- [ ] **ENG-228** — Sandbox observability without leakage: bounded structured logs, script line and
      instruction span, capability decisions, budget counters and redaction. Reports never include
      secrets, absolute owner paths or arbitrary binary memory.
- [ ] **ENG-229** — Integrate `/gamedebug` stage 04 with the sandbox and report the actual protocol,
      budgets, granted capabilities, termination reason and authored pre/post hashes.

**Acceptance:** an adversarial game cannot access DOM/network/files/providers, exceed a declared
budget, mutate authored bytes or survive Stop; a valid canonical game remains deterministic and
meets its existing play-runtime behaviour through the same broker.

---

### Phase 12 — Runtime sandbox verification and resilience  ·  `ENG-230…239`

*Second sandbox phase: continuously prove the boundary against malformed and hostile games.*

- [ ] **ENG-230** — Corpus of hostile scripts/bytecode/protocol messages: infinite loops, recursion,
      spawn/event/log floods, invalid opcodes, oversized strings, NaN/Infinity transforms, stale
      nonce, replayed/out-of-order frames and undeclared host calls.
- [ ] **ENG-231** — Property/fuzz tests for compiler output, bytecode decoder, protocol decoder and
      broker arguments. Every generated input has one of two outcomes: valid bounded execution or
      typed rejection—never panic/hang/undefined behaviour.
- [ ] **ENG-232** — Sandbox escape architecture gate scans Rust/TS/CSP/bundles for forbidden APIs
      and verifies the worker has no imported application object graph or direct IPC handle.
- [ ] **ENG-233** — Resource soak lane: repeated start/stop/restart, long deterministic play,
      level travel and fault storms; assert bounded worker count, listeners, timers, GPU resources,
      memory trend and report storage.
- [ ] **ENG-234** — Cross-platform equivalence fixtures on Windows/macOS/Linux/Web for checkpoint
      hashes and typed faults. Numeric tolerances are explicit; platform differences are recorded,
      never hidden by widening all assertions.
- [ ] **ENG-235** — Supply-chain boundary: runtime/compiler dependencies pinned and audited; worker
      bundle inventory and hash included in release provenance; no downloaded gameplay executable.
- [ ] **ENG-236** — Recovery and quarantine: a project with repeated sandbox termination opens in
      edit-only safe mode; Play and `/gamedebug --fix` explain the quarantine and require an
      explicit user action after the underlying blocker is removed.
- [ ] **ENG-237** — `/gamedebug release` makes sandbox, hostile corpus and soak summaries mandatory
      release evidence. Missing/incomplete sandbox evidence blocks release.
- [ ] **ENG-238** — Security-focused UI: visible capabilities and live budget meters, termination
      reason, safe restart, report export and no scary-but-meaningless “secure” badge.
- [ ] **ENG-239** — Final combined golden: deterministic AI generates a game, quality pipeline finds
      and repairs an intentional defect, hostile runtime probes are contained, the game completes,
      authored hashes/journal/build ledger match, and the exact commit passes every host lane.

**Acceptance:** the hostile corpus, fuzz/property suites, 30-minute soak and cross-platform
equivalence lanes pass; `/gamedebug release` includes their evidence; the combined golden proves
quality repair and sandbox containment on the exact release commit.

---

## Expanded Unreal-class audit and engine-first charter

This section incorporates the owner's broader engine audit without pretending that a long
feature list is an implementation. “Unreal-class” here means a serious, reusable engine
architecture and familiar production workflow; it does **not** mean copying Unreal, matching
every UE5 feature immediately, or using Epic/Unity branding and assets. Every external library
or source reference still needs an ADR, maintenance assessment and licence review.

The required AI path is:

```text
prompt → intent → capability retrieval → typed GameSpec → compose existing systems
       → bounded extension only for the missing part → engine documents/transactions
       → validate → sandboxed play → deterministic probes → evidence → editable result
```

The engine—not a prompt—must enforce this decision order:

1. configure an existing registered capability;
2. compose existing capabilities;
3. use a registered preset/template;
4. add a bounded project extension through a declared extension point;
5. only then propose reusable engine work, with an ADR, tests, cost and registry entry.

Directional target for a representative generated game: **70–85%** registered capabilities,
presets and configuration; **10–25%** graphs/composition; **under 5–10%** custom source. This
is an evaluation metric, not permission to misclassify generated code as configuration.

### Truth vocabulary for every subsystem

Future agents must report these dimensions separately. A subsystem is never simply “done”:

| Dimension | Proof required |
|---|---|
| Documented | authoritative contract/ADR exists and does not conflict with code |
| Implemented | production code exists; no placeholder or label-only surface |
| Tested | unit/fixture/integration evidence exercises its real contract |
| Editor-accessible | a human can discover, create, configure and diagnose it |
| AI-accessible | typed capability/query/action schemas expose it without source search |
| Runtime-proven | Play executes it and observations prove behaviour |
| Production-ready | budgets, platform/export, migration, recovery and release gates pass |

### Capability matrix — audited baseline and destination

Statuses are deliberately conservative and refer to the repository at this document's
2026-09-01 reconciliation point.

| Subsystem | Current status and evidence | Principal gap | Strategy / phase |
|---|---|---|---|
| Core scene/transactions | **PARTIAL, strongly tested** — scene graph, schemas, ULIDs, one transaction/journal/undo path, recovery | nested prefabs, sub-scenes, streaming, migrations and hot reload | BUILD/ADAPT · 16/21/22 |
| Rendering | **PARTIAL** — Three.js webview, GLTF, PBR maps, lights, weather, view modes | production render architecture, culling/LOD, shadows/GI/post/HDR/profiling | WRAP/ADAPT mature WebGPU/render primitives after ADR · 17 |
| Materials | **PARTIAL** — versioned PBR document and runtime binding | instances, layering, advanced lobes, node graph and hot reload | BUILD graph/compiler over runtime backend · 17/22 |
| Shaders | **PARTIAL** — versioned shader document and WGSL source reference | validated compilation pipeline, variants/cache/compute, graph and GPU diagnostics | WRAP compiler/reflection tooling · 17/22 |
| Physics | **PARTIAL, primitive** — bounded webview kinematic collision/controller | rigid bodies, joints, CCD, queries, vehicles, ragdolls, mature determinism | INTEGRATE/WRAP established Rust-capable physics after ADR · 18 |
| Character | **PARTIAL** — walk/run/jump controller, slope/step collision and possession | crouch/climb/swim/mantle/root motion/presets/network considerations | BUILD over selected physics · 18 |
| Skeletal animation | **STUB/PARTIAL schema visibility** — asset/query references exist | skinning runtime, graph, blend spaces, layers, retargeting, compression | INTEGRATE/ADAPT formats/runtime; BUILD graph · 19 |
| IK/FK/rigging | **MISSING** | solvers, constraints, control rig, editor/debug views | BUILD core solvers; evaluate reusable math crates · 19 |
| VFX/particles | **STUB** — weather has limited presentation effects | editable CPU/GPU graph, modules, pooling, LOD and debugging | BUILD graph; WRAP GPU primitives · 19 |
| Terrain/landscape | **MISSING** | chunks, LOD, layers, splines, water, foliage, streaming | BUILD orchestration; ADAPT licensed algorithms · 21 |
| Procedural generation | **PARTIAL, tested** — seeded grids/rings/scatter/rooms/corridors | graph/rule framework, terrain/roads/buildings/biomes and bake/edit flow | BUILD registry-backed generators · 21 |
| HUD/game UI | **PARTIAL, runtime-proven** — versioned document, 12 widgets, editor, bindings/actions | containers/rich text/menus/minimap/animation/responsive preview/presets | BUILD on current document · 13/20 |
| Input | **PARTIAL** — versioned actions/axes and runtime mapping | contexts, rebinding, chords, controller/touch/vibration/accessibility | BUILD platform adapters · 18 |
| Cameras | **PARTIAL** — editor views, possession/eject and camera component | reusable first/third/top/racing/cinematic rigs, blend/collision/shake | BUILD presets/components · 18 |
| Lighting/environment | **PARTIAL** — directional/point/spot, sky/fog/weather presets | area/probes/baked options, volumetrics, day/night/material/audio coupling | renderer-backed BUILD · 17 |
| Navigation | **MISSING** | navmesh, costs, links, dynamic obstacles, crowd/steering/debug | INTEGRATE/WRAP suitable navigation library after ADR · 20 |
| Gameplay AI | **MISSING** | state/behaviour/utility systems, blackboard, perception and encounters | BUILD data/graph systems over navigation · 20/22 |
| Gameplay framework | **STUB/PARTIAL** — variables/events, script host API, basic triggers | reusable health/inventory/combat/objective/save components and presets | BUILD registered components · 20 |
| Weapons/vehicles | **MISSING** | reusable weapon families, wheel/vehicle physics, cameras/audio | BUILD presets over physics/gameplay/audio · 20 |
| Audio | **STUB schema only** | playback/import/spatialisation/mixers/zones/streaming/profiling | INTEGRATE/WRAP mature audio backend after ADR · 19 |
| Scripting | **PARTIAL, tested** — Rust compiler to bounded bytecode + webview VM | worker isolation, hot reload, profiling and broader safe API | BUILD on ADR-0030 · 11/12/16 |
| Visual scripting | **MISSING** | typed event/behaviour graph, compiler, debugger and human editor | BUILD on action/capability schemas · 22 |
| Prefabs/blueprints | **PARTIAL** — versioned prefab capture/instantiate/propagation | nesting, variants, exposed parameters, graph composition and conflict UX | BUILD on current prefab model · 22 |
| Asset pipeline/browser | **PARTIAL** — index, metadata/licence, refs, import copy, dependency queries, basic browser | conversion/reimport/thumbnails/settings/cache/rename safety/streaming | WRAP importers; BUILD orchestration/UI · 17 |
| Editor UX | **PARTIAL and over-dense** — strong Outliner/Details/viewport/content/log primitives | reduce toolbar overload, fixed hierarchy, contextual modes, coherent bottom drawer | SIMPLIFY before adding panels · 13 |
| Runtime | **PARTIAL** — webview simulation, compiled scripts, HUD, level travel | isolated worker/kernel, scheduling, async loading, lifetime and platform contracts | BUILD/ADAPT · 11/12/16 |
| Large worlds/streaming | **MISSING** | sublevels, async cells, HLOD/terrain/foliage streaming | BUILD after renderer/runtime asset lifetime · 21 |
| Saving | **EDITOR PARTIAL; GAME SAVE MISSING** — authored recovery exists | versioned runtime save/checkpoint/persistent state and migration | BUILD · 23 |
| Networking | **MISSING** | authority/replication/RPC/prediction architecture | DESIGN now, BUILD after deterministic runtime/save identity · 23 |
| Debug/profiling | **PARTIAL** — typed console, source links, stats, `/debug`, `/gamedebug` slice | frame/GPU/memory/event/AI/nav/animation inspectors and captures | BUILD adapters over subsystem telemetry · 24 |
| Testing/performance | **PARTIAL, good headless base** — broad Rust/UI tests and 1k headless budget | real GPU/device corpus, subsystem stress scenes, soak and regression dashboard | BUILD evidence lanes · 24 |
| AI control | **PARTIAL, strong scene seam** — actions, batches, queries, context budget, autonomy loop | unified capability registry, GameSpec planner, mechanic contracts and all-subsystem coverage | BUILD registry/retrieval/planner · 14/15 |

### Handoff snapshot — where the plan actually is now

- **Proven foundation:** Phases 0–7 are substantially implemented as recorded in their ticket
  rows; Phase 7 is complete. Phase 8 still lacks reference-GPU and host-launch proof.
- **Active implementation:** Phase 9. ADR-0032 and the first `/gamedebug` slice exist;
  ENG-201/206/207/208 remain partial for schema goldens, complete build/shader composition,
  timings/retention/atomic latest replacement and full/runtime-linked navigation.
- **Specified, not implemented:** remaining Phase 9, all of Phases 10–12, and all expansion
  Phases 13–24. The capability matrix above is a gap map, not a shipped-feature list.
- **Immediate order:** finish the Phase 9 static/report contract → Phase 11 sandbox foundation →
  Phase 9/10 runtime quality loop → Phase 12 resilience. Phase 13 UI simplification may proceed
  in parallel because it must preserve behaviour. Phase 14 registry precedes new subsystem
  breadth; do not start terrain, VFX or networking as isolated features before it.
- **UI warning:** do not add another permanent toolbar control or panel. Until Phase 13 lands,
  new features enter the command palette, Inspector or existing bottom surfaces.

---

### Phase 13 — Minimal editor reset and calm Unreal/Unity workflow · `ENG-240…254`

*The engine currently exposes too many equal-weight controls at once. This phase removes
visual noise before adding another subsystem panel. It changes information architecture, not
engine truth.*

#### Canonical shell

```text
┌ Project / Scene / Save ───────── Play · Pause · Stop ─────── Build · AI · More ┐
├── Modes ─┬────────────── Viewport (context toolbar) ─────────┬── Inspector ───┤
│ Select   │                                                   │ selected item   │
│ Scene    │                                                   │ properties      │
│ HUD      │                                                   │                 │
│ Material │                                                   │                 │
├──────────┴───────────────────────────────────────────────────┴─────────────────┤
│ Content | Output | Problems | AI Activity | Game Debug                ▴       │
└────────────────────────────────────────────────────────────────────────────────┘
```

The **Outliner** is the left panel in Scene mode; mode switching may replace its contents,
not create another permanent panel. The **Inspector** is the sole persistent right panel.
Content, logs, problems, AI activity and game-debug reports share one bottom drawer with tabs.
The viewport always owns the largest area.

- [ ] **ENG-240** — Record a before-state usability fixture: screenshots at 1366×768,
      1440×900 and 1920×1080; visible-control count; toolbar wrapping; viewport percentage;
      five task timings; keyboard path and axe result. Preserve it for comparison.
- [x] **ENG-241** (codex, 2026-09-01) — Write ADR-0034: fixed/preset shell first, no floating-window/docking
      framework yet. Supersede ENG-140's docking-first acceptance; advanced docking may return
      only after the fixed shell passes the task and density budgets.
- [~] **ENG-242** (codex, 2026-09-01 — code and source guard ship; 1440 screenshot/count pending) — Replace the current multi-row everything-toolbar with three quiet zones:
      Project/Scene/Save; centred Play/Pause/Stop; Build, AI status and one **More** menu.
      At 1440 px, no more than nine visible actionable controls and no wrapping.
- [x] **ENG-243** (codex, 2026-09-01) — Move transform tools into a compact viewport strip: Select/Move/Rotate/
      Scale, World/Local, one snap control, view mode and Show. Put camera speed/FOV/screen
      percentage/shading variants/maximise in contextual popovers, preserving shortcuts.
- [x] **ENG-244** (codex, 2026-09-02) — Move Restart, Step, time scale, Eject, Break-on-error and live metrics into
      a Play options popover/status strip shown only during Play. Stop remains one-click.
- [~] **ENG-245** (codex, 2026-09-01 — AI status/capabilities moved; contextual weather Inspector remains) — Replace always-visible Agent mode/capabilities/weather with one AI status
      button and context-owned Inspector sections. Destructive pending approval remains visible;
      hiding controls must never hide permission state or active execution.
- [~] **ENG-246** (codex, 2026-09-01 — Scene/HUD rail ships; later document modes remain) — One left mode rail: Select/Scene/HUD/Material/Animation/Game. Each mode has
      one clear primary task and swaps contextual left/centre/right content without duplicating
      Outliner, Inspector or toolbars.
- [~] **ENG-247** (codex, 2026-09-02 — shared tabs, `Ctrl+J`, per-project open-tab state and automatic error raising ship; resizable height and game-debug report raising remain) — Consolidate Content Drawer and Output Log into the bottom drawer tabs above;
      add Problems and Game Debug. Remember height/open tab per project, not transient report
      contents. `Ctrl+J` toggles it; errors may raise its tab without stealing keyboard focus.
- [ ] **ENG-248** — Simplify Outliner rows to disclosure, type icon, name and quiet state glyphs.
      Search is always available; filter chips live behind Filter; row actions appear on hover,
      focus or context menu. Complete organiser folders without mixing them with parent transforms.
- [ ] **ENG-249** — Simplify Inspector hierarchy: identity, Transform, then collapsed schema
      categories. Show changed/AI-authored values subtly; advanced/raw JSON stays behind Advanced.
      One primary Add Component action, schema search and inline validation.
- [~] **ENG-250** (codex, 2026-09-01 — new shell is restrained; legacy/theme token cleanup remains) — Establish one restrained engine visual system: neutral charcoal surfaces,
      one amber product accent, semantic red/yellow/green only for state, 4/8 px spacing rhythm,
      compact 28/32 px controls, one border language, minimal shadows and no decorative glow in
      editor chrome. Existing appearance themes may vary tokens, not hierarchy or density.
- [~] **ENG-251** (codex, 2026-09-01 — explicit 1200/900 degradation ships; Inspector tab/drawer remains) — Responsive degradation: under 1200 px the Inspector becomes a tab/drawer;
      under 900 px use focused single-panel mode. Never horizontally scroll the main toolbar or
      leave the viewport below 50% of available workspace width.
- [ ] **ENG-252** — Complete state design for every shell zone: loading, empty, error, populated,
      disabled-by-mode, no-selection, multi-selection, Play and narrow viewport. Empty states offer
      exactly one primary next action plus optional help.
- [~] **ENG-253** (codex, 2026-09-01 — More exposes both palettes and existing handlers; full registry audit remains) — Command palette becomes the complete expert path; toolbar/menu items and
      shortcuts reference the same command registry. Removing a visible button may not remove its
      keyboard, palette, accessibility or AI route.
- [ ] **ENG-254** — Visual/usability golden: the five common tasks—select/edit transform, add an
      entity, change material, run/stop, inspect `/gamedebug`—complete without documentation,
      toolbar wrap or hidden state. Compare before/after viewport area and interaction count;
      axe remains zero serious/critical and reduced motion is honoured.

**Acceptance:** at 1440×900 the default layout is one calm toolbar, one left work context, a
dominant viewport, one Inspector and one collapsed-by-default bottom drawer. No feature is lost,
the five task paths are no slower, and advanced controls remain reachable through context,
palette or shortcuts. Do not call this complete from a CSS screenshot alone.

**Slices shipped:** ADR-0034; a quiet primary toolbar; one Scene/HUD mode rail; compact
viewport transform/snap/shading/camera/Show/options strip; focused AI permission menu; More menu
for palettes, edit, drawer/log, maximise and reload; Play-only advanced simulation options and
metrics; one collapsed-by-default bottom drawer for Content, real Output, Problems, AI Activity,
Game Debug and Build Targets; `Ctrl+J`; explicit narrow-width degradation; and four source guards.
Existing handlers and shortcuts are preserved. ENG-242 stays partial until
the real Tauri Engine state is captured/measured at 1440 px; browser-only preview cannot enter a
project-backed Engine pane. Resizable drawer-height persistence and game-debug report raising, responsive Inspector,
the full mode set, before/after task timing and complete visual-system cleanup remain open.

---

### Phase 14 — Engine Capability Registry · `ENG-255…269`

- [ ] **ENG-255** — ADR-0035 defines `bhippi-capability@1`, ownership, version compatibility,
      extension registration and the difference between capability, component, preset and tool.
- [ ] **ENG-256** — Registry entry includes id/category/version, purpose, typed inputs/outputs,
      properties, operations, dependencies/conflicts, runtime requirements, cost class, platform
      support, editor route, examples, limitations, extension points and validators/debuggers.
- [ ] **ENG-257** — Generate core entries from authoritative Rust schemas where possible; hand-
      written metadata may enrich but never contradict component/action/query/host definitions.
- [ ] **ENG-258** — Merge component registry, actions, queries, templates, HUD widgets, weather,
      script hosts and build targets into one read-only catalogue without moving their owners.
- [ ] **ENG-259** — Stable relations: requires, conflicts, composes-with, supersedes, provides,
      consumes, test-with and editor-for. Cycles and dangling ids block registry build.
- [ ] **ENG-260** — Capability maturity vector uses the seven truth dimensions above plus
      platform/budget evidence; “available” requires the task's needed dimensions, not one flag.
- [ ] **ENG-261** — Compact retrieval cards and deep detail queries; token budgets measured at
      50, 500 and 5,000 capabilities. Never inject the complete registry into a turn.
- [ ] **ENG-262** — Search by intent, category, compatible component, platform and cost with a
      deterministic lexical baseline before optional embeddings.
- [ ] **ENG-263** — Preset registry for controllers, cameras, enemies, HUDs and game templates;
      each preset expands to versioned capability configuration and remains manually editable.
- [ ] **ENG-264** — Extension manifest declares dependencies, permissions, config, runtime/editor/
      AI exposure, performance cost, platform support and version. Unknown permissions block.
- [ ] **ENG-265** — Inspector and Add menus are registry projections grouped by human task; no
      second TypeScript catalogue. Capability detail explains limits and test/debug route.
- [ ] **ENG-266** — AI query/action API exposes registry search/detail/compatibility/validate;
      every returned action uses the existing transaction and capability-permission path.
- [ ] **ENG-267** — Architecture test proves every public engine component/action/query/preset
      has a registry entry and every entry resolves to real code plus at least one validator.
- [ ] **ENG-268** — Licence/provenance fields for bundled presets and integrations; unavailable
      platform/dependency states are explicit and never silently substituted.
- [ ] **ENG-269** — Registry golden: a third-person survival request retrieves a bounded relevant
      set and rejects a hallucinated capability with alternatives and extension guidance.

**Acceptance:** the AI and editor discover the same versioned capability truth; the golden request
retrieves only relevant registered systems; no public capability is orphaned or prompt-only.

---

### Phase 15 — Intent, GameSpec and composition planner · `ENG-270…279`

- [ ] **ENG-270** — `bhippi-game-spec@1`: genre, player loop, mechanics, world, actors, UI,
      quality/platform/budget constraints and acceptance mechanics; unknown major blocks.
- [ ] **ENG-271** — Intent parser produces GameSpec facts with confidence and questions only for
      decisions that materially alter the game; it does not emit implementation code.
- [ ] **ENG-272** — Planner resolves each requirement through existing → partial+extension →
      composition → bounded new extension and records why a lower-cost existing option was unused.
- [ ] **ENG-273** — Compatibility solver checks dependency/conflict/platform/performance relations
      before writes; failure returns alternatives from the registry.
- [ ] **ENG-274** — Composition plan is a typed DAG of capabilities, configs, documents, actions,
      test scenarios and budgets with a human-readable preview.
- [ ] **ENG-275** — Cost/token estimator blocks plans exceeding target frame/memory/content/turn
      budgets before generation and offers scoped reductions.
- [ ] **ENG-276** — One approved plan executes as labelled transaction batches and content actions;
      all outputs remain normal scene/HUD/material/prefab/graph documents.
- [ ] **ENG-277** — New extension flow scaffolds the narrow manifest/API/tests/docs/registry entry;
      no arbitrary engine-source mutation in a normal game-generation turn.
- [ ] **ENG-278** — Mechanic Contract format maps promise → setup → deterministic probes → expected
      evidence, and feeds `/gamedebug` scenarios and repair findings.
- [ ] **ENG-279** — Golden games prove registry-first composition and report configuration/graph/
      source percentages against the directional token-efficiency target.

**Acceptance:** representative FPS, platformer, survival and puzzle prompts produce reviewable
GameSpecs and plans that reuse registered systems, execute through existing write paths and arrive
with deterministic mechanic tests before claiming completion.

---

### Phase 16 — Runtime kernel and subsystem contracts · `ENG-280…289`

- [ ] **ENG-280** — ADR settles editor/webview/runtime/module-worker boundaries after sandbox work.
- [ ] **ENG-281** — Fixed-step simulation scheduler with explicit system ordering/dependencies.
- [ ] **ENG-282** — Resource/asset lifetime handles, async loading and cancellation; no raw paths on
      the frame path.
- [ ] **ENG-283** — Runtime world/entity API shared by script, physics, animation, navigation,
      gameplay, audio and tests without exposing authored mutation.
- [ ] **ENG-284** — Event bus with typed bounded queues, ordering and backpressure.
- [ ] **ENG-285** — Job/worker contract for safe parallel work; deterministic lane stays available.
- [ ] **ENG-286** — Hot reload swaps validated resources/scripts/config at safe points with rollback.
- [ ] **ENG-287** — Per-system CPU/memory counters and trace spans feed one profiler schema.
- [ ] **ENG-288** — Platform capability layer reports actual Windows/macOS/Linux/Web support.
- [ ] **ENG-289** — Kernel soak/restart/deterministic replay fixtures become prerequisites below.

### Phase 17 — Production rendering, assets, materials and shaders · `ENG-290…304`

- [ ] **ENG-290** — Renderer ADR chooses BUILD/WRAP/ADAPT backend from measured prototypes,
      platform reach, maintenance and licences; no renderer rewrite starts from aesthetics.
- [ ] **ENG-291** — Render graph/passes, resource lifetime and debug labels.
- [ ] **ENG-292** — Frustum/occlusion culling, batching, instancing and indirect path where supported.
- [ ] **ENG-293** — Mesh LOD/HLOD and texture/mesh streaming with visible residency diagnostics.
- [ ] **ENG-294** — Production shadows, probes/IBL, AO, reflections and declared GI options.
- [ ] **ENG-295** — Post stack: exposure, tone map, bloom, colour grade, AA/upscale and HDR policy.
- [ ] **ENG-296** — Atmosphere/cloud/fog/decals/render layers/targets integrated with weather.
- [ ] **ENG-297** — Material instances, parameter overrides, advanced lobes and layered materials.
- [ ] **ENG-298** — Typed material graph compiles to the runtime representation; same graph is AI
      action/query accessible and manually editable.
- [ ] **ENG-299** — Shader compiler/reflection, includes, variants/permutations, cache and hot reload.
- [ ] **ENG-300** — Compute shader contract and strict platform capability reporting.
- [ ] **ENG-301** — Import/conversion/reimport pipeline for required mesh/texture/HDR formats with
      unit/axis/material report, deterministic cache and licence metadata.
- [ ] **ENG-302** — Real thumbnails/previews, safe rename/move/dependency rewrite and unused search.
- [ ] **ENG-303** — GPU capture/profiling/debug views for passes, resources and shader errors.
- [ ] **ENG-304** — Standard render scenes gate fps/1% low, GPU/CPU ms, VRAM, draw calls and quality.

### Phase 18 — Physics, character, input and cameras · `ENG-305…319`

- [ ] **ENG-305** — Physics ADR benchmarks mature candidates; prefer INTEGRATE/WRAP over rebuilding
      rigid-body fundamentals, with licence/platform/determinism evidence.
- [ ] **ENG-306** — Bodies/colliders/materials/layers/masks/triggers/CCD and compound/convex support.
- [ ] **ENG-307** — Forces, impulses, damping, queries/casts/overlaps and debug visualisation.
- [ ] **ENG-308** — Constraints/joints/springs/ropes plus lifecycle and stability tests.
- [ ] **ENG-309** — Character controller v2: crouch, slide, swim, climb, ladder, mantle and root motion.
- [ ] **ENG-310** — Character presets: first/third/platformer/top-down/flying with exposed parameters.
- [ ] **ENG-311** — Input contexts, chords, rebinding, controller/touch/vibration and UI switching.
- [ ] **ENG-312** — Camera rigs/presets, blends, modifiers, collision, shake and target tracking.
- [ ] **ENG-313** — Vehicle foundation: wheel/suspension/traction/gears/brake plus camera/audio seams.
- [ ] **ENG-314** — Buoyancy/destruction/ragdoll remain separate measured capability packs.
- [ ] **ENG-315** — Deterministic/fixed-step physics lane with explicit tolerances.
- [ ] **ENG-316** — Character obstacle-course corpus across every controller preset.
- [ ] **ENG-317** — Input-device/context/rebinding/accessibility matrix.
- [ ] **ENG-318** — Camera blend, collision and occlusion fixtures.
- [ ] **ENG-319** — Physics stress scenes and CPU/memory stability budgets.

### Phase 19 — Animation, rigging, VFX and audio · `ENG-320…334`

- [ ] **ENG-320** — Skeleton, bone hierarchy, skinning and clip import/runtime.
- [ ] **ENG-321** — Animation compression, blending, blend spaces and pose cache.
- [ ] **ENG-322** — State/animation graphs, layers, masks and additive animation.
- [ ] **ENG-323** — Events/notifies, root motion, retargeting and montage-like sequences.
- [ ] **ENG-324** — Animation editor/debugger and 100/500-character budgets.
- [ ] **ENG-325** — FK and two-bone IK with deterministic pose fixtures.
- [ ] **ENG-326** — CCD/FABRIK, foot/hand/look/aim constraints and pole targets.
- [ ] **ENG-327** — Control-rig graph, constraints and runtime rig editing.
- [ ] **ENG-328** — Editable rig layers, solver diagnostics and regression corpus.
- [ ] **ENG-329** — Typed CPU/GPU VFX graph with emitter/module/curve primitives.
- [ ] **ENG-330** — Collision, ribbons/trails/beams/decals/lights/events/sub-emitters.
- [ ] **ENG-331** — VFX pooling, LOD, overdraw diagnostics and GPU/CPU budgets.
- [ ] **ENG-332** — Audio import, playback, streaming and device lifecycle.
- [ ] **ENG-333** — Spatial attenuation, occlusion, reverb and audio zones.
- [ ] **ENG-334** — Mixers/buses/effects/events/music/voice priority and performance tests.

### Phase 20 — Gameplay framework, navigation and gameplay AI · `ENG-335…349`

- [ ] **ENG-335** — Health/damage/stamina/mana/shield components, events and HUD bindings.
- [ ] **ENG-336** — Inventory/equipment/items, pickups and persistence contract.
- [ ] **ENG-337** — Interaction, doors, switches, checkpoints and respawn.
- [ ] **ENG-338** — Teams/factions/score/objectives/quests/dialogue and win/lose state.
- [ ] **ENG-339** — Gameplay presets plus mechanic-contract corpus for the above.
- [ ] **ENG-340** — Hitscan/projectile/melee weapon core and damage integration.
- [ ] **ENG-341** — Recoil/spread/reload/ammo/switching/falloff/attachments.
- [ ] **ENG-342** — Weapon animation/VFX/audio/editor presets and tests.
- [ ] **ENG-343** — Navmesh generation/update, areas/costs and off-mesh links.
- [ ] **ENG-344** — Path queries, dynamic obstacles, avoidance/crowd and flying navigation.
- [ ] **ENG-345** — Navigation editor/debug views and deterministic path corpus.
- [ ] **ENG-346** — Gameplay-AI state machine, blackboard and perception.
- [ ] **ENG-347** — Behaviour tree and utility-AI graphs with debugger.
- [ ] **ENG-348** — Patrol/chase/cover/combat/flee/investigation/squad behaviours.
- [ ] **ENG-349** — Spawning/encounter management presets and crowd performance gates.

### Phase 21 — Terrain, procedural worlds and streaming · `ENG-350…364`

- [ ] **ENG-350** — Chunked heightfield terrain document/runtime and editable bake model.
- [ ] **ENG-351** — Terrain LOD, collision, normal generation and streaming seam.
- [ ] **ENG-352** — Landscape layers, painting, masks, materials and erosion/noise tools.
- [ ] **ENG-353** — Splines, roads, rivers, lakes/ocean and bridge/intersection rules.
- [ ] **ENG-354** — Terrain editor, manual overrides and deterministic regeneration tests.
- [ ] **ENG-355** — Biome rules and deterministic foliage/grass/tree/rock scatter.
- [ ] **ENG-356** — Foliage culling, impostors, pooling and density/overdraw budgets.
- [ ] **ENG-357** — Procedural settlement/building/road integration with biome constraints.
- [ ] **ENG-358** — Alpine-valley golden with editable layers, roads and village outputs.
- [ ] **ENG-359** — Versioned procedural graph with seed/noise/spline/grid/graph primitives.
- [ ] **ENG-360** — Grammars, rules and WFC-like room/corridor/building/city composition.
- [ ] **ENG-361** — Loot/encounter generators, graph debugger and provenance/bake workflow.
- [ ] **ENG-362** — Sub-scenes and versioned streaming-cell/world-partition document.
- [ ] **ENG-363** — Async level/terrain/foliage streaming, cancellation and origin strategy.
- [ ] **ENG-364** — HLOD and large-world edit/replay/loading/memory stress fixtures.

### Phase 22 — Visual graphs, prefab evolution and plugins · `ENG-365…374`

- [ ] **ENG-365** — Versioned typed behaviour/visual-scripting graph document.
- [ ] **ENG-366** — Graph compiler to safe bytecode/actions with static type/cycle checks.
- [ ] **ENG-367** — Breakpoints, trace, watch values and deterministic graph tests.
- [ ] **ENG-368** — Minimal node editor; AI and human edit the identical graph document.
- [ ] **ENG-369** — Nested prefabs and dependency-safe update propagation.
- [ ] **ENG-370** — Variants, exposed parameters, overrides and composition inheritance.
- [ ] **ENG-371** — Prefab conflict UX, migration, replication metadata and golden fixtures.
- [ ] **ENG-372** — Versioned plugin/extension manifest, permissions and lifecycle.
- [ ] **ENG-373** — SDK for components/importers/editor panels/render/physics features.
- [ ] **ENG-374** — Capability packs, install/update/uninstall recovery and hostile-plugin tests.

### Phase 23 — Runtime saves, networking, platforms and export · `ENG-375…384`

- [ ] **ENG-375** — Versioned runtime save/checkpoint and persistent-world schema.
- [ ] **ENG-376** — Async save/load, atomicity, corruption recovery and rollback.
- [ ] **ENG-377** — Save migration fixtures and forward/backward compatibility policy.
- [ ] **ENG-378** — Cloud-provider-neutral save extension seam and conflict contract.
- [ ] **ENG-379** — Networking ADR: authority, tick, identity, transport and threat model.
- [ ] **ENG-380** — Replication/RPC/interpolation foundation with deterministic fixtures.
- [ ] **ENG-381** — Prediction/reconciliation and session/lobby extension seams.
- [ ] **ENG-382** — Reproducible Windows/macOS/Linux/Web export pipelines and doctor.
- [ ] **ENG-383** — Packaging/signing hooks, dependency/licence inventory and crash symbols.
- [ ] **ENG-384** — Install/launch/upgrade/rollback smoke lanes on available hosts.

### Phase 24 — Production profiling, debugging and release proof · `ENG-385…399`

- [ ] **ENG-385** — Unified CPU/GPU/memory/event trace schema and capture lifecycle.
- [ ] **ENG-386** — Physics/navigation/animation/gameplay-AI inspectors and debug draw.
- [ ] **ENG-387** — Render/shader/resource inspectors and GPU capture integration.
- [ ] **ENG-388** — Crash bundle with redaction, symbols, replay metadata and report export.
- [ ] **ENG-389** — Compact AI observation queries over every profiler/debug surface.
- [ ] **ENG-390** — 1k/10k static and 1k dynamic benchmark scenes.
- [ ] **ENG-391** — 100/500 animated-character and AI-crowd benchmark scenes.
- [ ] **ENG-392** — Large terrain/streaming/loading benchmark scenes.
- [ ] **ENG-393** — Heavy VFX/lighting/HUD/physics benchmark scenes.
- [ ] **ENG-394** — Regression floors for fps, 1% low, subsystem ms, RAM/VRAM, draws and load time.
- [ ] **ENG-395** — Serialization, migration and public engine API contract suites.
- [ ] **ENG-396** — Deterministic scene/physics/gameplay/mechanic/integration and screenshot suites.
- [ ] **ENG-397** — Benchmark mutation, long-soak and fault-recovery suites.
- [ ] **ENG-398** — Documentation/capability matrix regenerated from real registry evidence; each
      row reports all seven truth dimensions and exact platform/budget limitations.
- [ ] **ENG-399** — Final production golden generates, manually edits, tests, repairs, saves,
      exports and launches representative games on available hosts; release is blocked by any
      unsupported required capability, stale evidence or authored-state mismatch.

**Expanded-track acceptance:** no subsystem is promoted by a UI label or document alone. Each
phase closes only when its editor, AI, runtime, tests, budgets and target-platform evidence meet
the named acceptance conditions and the capability matrix is updated from code.

#### Final Git publication gate

After — and only after — every required ticket and the Phase 8–24 acceptance evidence above are
complete, publish the verified project to:

`https://github.com/memegyanfactory-gif/bhippiADE`

Before pushing:

1. Confirm the local project is a Git checkout whose intended remote resolves to that exact
   repository; never initialise or replace unrelated history silently.
2. Fetch first and inspect divergence. Preserve remote work; do not force-push, reset or
   overwrite history unless the owner separately authorises that destructive operation.
3. Review the complete diff for secrets, generated caches, captures, build artefacts and
   local `.bhippi/` state. Only source, required fixtures and authoritative docs belong in
   the release commit.
4. Re-run the Definition of Done and Phase 8 gates on the exact commit being pushed. Record
   the commit SHA and verification summary in `PROGRESS.md`.
5. Push the completed branch, then verify the remote commit and CI status. A successful
   local `git push` without remote/CI verification does not close this gate.

**Do not publish this planning-only checkpoint as “everything done.”** Open `[ ]`, `[~]` or
resolvable `[!]` tickets mean the implementation is not ready for the final Git update.

---

## 5. Rules the engine must enforce (not "tell the AI about")

Every one of these is a gate that **blocks**. Prompt text is a courtesy; the check is code.

| Rule | Enforced in | Phase |
|---|---|---|
| Scene writes only through `EngineTransaction` | `bhippi-app::engine` — no other write path exists | 0 |
| Every transaction journaled with actor + label | `bhippi-db::EngineRepo::append_transaction` | 0 |
| The webview never owns authored document truth; its permitted picture/runtime math is exactly ADR-0028/M15 | architecture grep + review of `ui/src/engine/` | 0/5/6 |
| Entity ids are ULIDs; parents exist; no cycles | `SceneDocument::validate` (already) | — |
| Component payloads validate against the registry | `schema::validate_component` (already) | — |
| Asset refs resolve to indexed assets | `ENG-128` gate | 2 |
| No unlicensed asset in a Release build | `bhippi-engine-build::preflight` (INV-074) | 2 |
| `default_scene` is Main; every `levels[]` path exists; HUD path exists | `ENG-128` gate | 2 |
| Weather id ∈ the eight presets | `schema.rs` enum (already) + `ENG-128` | 2 |
| HUD documents validate against the widget registry | `bhippi-engine::hud` | 3 |
| Material/shader documents validate | `bhippi-engine::material` | 2 |
| Play never mutates authored scene files | `ENG-171` snapshot/restore + test | 6 |
| Gameplay source is compiled in Rust; the webview executes bounded bytecode without `eval`/`Function` | `bhippi-engine::script` + architecture grep (INV-082) | 6 |
| Screenshot/playtest requests are capability-gated, size/time/step bounded, and fail loudly when no pane answers | `engine/observation.rs` + request validators | 7 |
| Dynamic engine context stays within its token budget; deeper facts require retrieval | `chat.rs::cap_engine_facts` + Token Engine samples | 7 |
| Agent capability limits honoured | `ENG-190` | 7 |
| `/gamedebug` always runs the versioned engine-owned stage graph; the model cannot skip, reorder or self-grade stages | `bhippi-engine::game_debug` + command parser golden tests | 9 |
| A game-generation quality claim names the rubric version, corpus case, evidence and independent evaluator result | `bhippi-engine-quality` corpus/evaluator + release ledger | 9/10 |
| Quality regressions block on the committed baseline and cannot be hidden by averaging unrelated cases | per-case floors + aggregate floor + baseline-delta gate | 10 |
| Generated gameplay has a bounded, deterministic seed/input/clock envelope for evaluation | game-debug exercise adapter + replay fixture | 10 |
| Runtime code receives declared capabilities only; filesystem, network, process and secret access default deny | sandbox broker/policy — no direct host handle in the guest | 11 |
| CPU steps, wall time, memory, output, spawn depth and host-call rate are hard budgets | sandbox supervisor + typed budget faults | 11 |
| A worker/process boundary is never described as a security boundary without an OS containment lane proving it | sandbox backend capability report + platform hostile corpus | 11/12 |
| Sandbox escape, confused-deputy and denial-of-service fixtures fail closed and leave authored bytes unchanged | adversarial corpus + pre/post tree hash | 12 |
| Technology/AI-only topics; robots/paywalls obeyed; no unlicensed image | existing repo gates | — |

---

## 6. The chat ↔ engine contract, after this plan

```
USER: "make the warehouse darker and put a health bar top-left"
  │
  ├─ chat.rs assembles the turn:
  │     CHAT_SYSTEM + rules + prompts/chat-engine.md v9
  │     + engine context (retrieval): open scene, selection, nearby entities, recent errors
  │
  ├─ model calls READ tools ......... engine_query_scene_view, find_entities{has:"Light"},
  │                                    get_entity, search_assets{kind:"font"}
  │
  ├─ model emits ONE batch ........... engine_apply_batch {
  │                                      label: "darken warehouse + health bar",
  │                                      actions: [ set_weather{overcast},
  │                                                 patch_component{Sun, Light, intensity 0.6},
  │                                                 set_ambient{[.10,.11,.13]},
  │                                                 hud_add_widget{progress_bar, top_left, …} ] }
  │
  ├─ app applies it ................. ONE EngineTransaction · ONE journal row · ONE undo step
  │                                    (Ask mode: plan card first — Approve / Reject / Edit)
  │
  ├─ result envelope back to model .. per-action ok/err + hint + schema excerpt on failure
  │
  ├─ model VERIFIES ................. engine_screenshot + get_entity → confirms or repairs
  │
  └─ UI ............................. EngineTransactionApplied event → viewport patches the
                                       touched entities · Outliner/Details refresh · toast
                                       "Agent changed 4 things · Undo"
USER then: opens hud_main.hud.json, drags the bar, retypes the label, hits Play — and it runs.
```

`/gamedebug` is a second, deliberately non-conversational entry point into that same engine
truth. The command parser selects `quick`, `full` or `release`; the engine then owns discovery,
validation, compilation, sandboxing, exercise, inspection, observation, scoring and reporting.
The provider receives the final JSON report as evidence for a later repair turn, but it cannot
edit stage status, invent a pass, suppress a blocker or write directly during the diagnostic
run. `/gamedebug --fix` is a separate capability-gated repair transaction followed by a fresh
diagnostic run; the report records both run ids and the exact transaction id.

---

## 7. Files this plan touches

| Concern | Path |
|---|---|
| Scene/transaction/schema domain | `crates/bhippi-engine/src/{document,transaction,action,schema,scaffold,query,api}.rs` |
| Engine domain additions | `crates/bhippi-engine/src/{hud,hud_action,material,prefab,procedural,gates,mesh,weather,compose,script,capability}.rs` |
| Engine app seam + sessions/bridge | `crates/bhippi-app/src/engine/{mod,session,bridge,query_bridge,content,hud_session,observation}.rs` |
| Chat bridge | `crates/bhippi-app/src/chat.rs`, `prompts/chat-engine.md` |
| Journal persistence | `crates/bhippi-db/migrations/*`, `crates/bhippi-db/src/engine.rs` |
| Viewport + runtime (ADR-0028) | `ui/src/engine/{EngineViewport,renderResources,playRuntime,scriptVm}.ts*`; retired JSON-RPC design remains in `crates/bhippi-engine-viewport/src/protocol.rs` |
| Build gates | `crates/bhippi-engine-build/src/lib.rs` |
| Fixed game-debug contract and static stages | `crates/bhippi-engine/src/game_debug.rs` |
| Game-debug orchestration/report storage | `crates/bhippi-app/src/game_debug.rs`, `crates/bhippi-app/src/chat.rs` |
| Quality corpus, rubric and evaluator | `crates/bhippi-engine-quality/{src,tests,fixtures}/`, `tests/fixtures/engine/quality/` (Phase 9 creates the crate only after the architecture guard is updated) |
| Runtime sandbox policy/broker/backends | `crates/bhippi-engine-sandbox/{src,tests}/`, `tests/fixtures/engine/sandbox/` (Phase 11; backend selection requires its ADR) |
| Minimal editor shell and modes | `ui/src/engine/EngineView.tsx`, existing `Engine{Hierarchy,Inspector,ContentDrawer,OutputLog,HudEditor,CommandPalette}.tsx`, `ui/src/styles/workbench.css`; consolidate these, do not create duplicate panels |
| Capability registry / GameSpec / planner | planned Rust domain modules/crates per ADR-0035 and architecture review; no TypeScript catalogue or prompt-only registry |
| Advanced subsystem implementations | planned engine/runtime crates only after their phase ADR and architecture-edge update; editor panels remain projections of typed Rust schemas |
| Bindings | `ui/src/lib/ipc.ts` (regenerate whenever the command surface changes) |
| Trackers | `docs/PROGRESS.md`, `docs/08-BUILD-ORDER.md`, this file |

---

## 8. Dependency order (do not reorder casually)

```
Phase 0  one write path ─┬─> Phase 1  AI bridge ─┬─> Phase 2  content generation ──┐
                         │                       └─> Phase 3  HUD system ──────────┤
                         └─> Phase 4  Unreal UX (parallel-safe after 0)            │
                                                                                   ▼
                             Phase 5  rendering truth ──────────────> Phase 6  play mode
                                                                                   │
                                                                                   ▼
                                                                    Phase 7  AI autonomy
                                                                                   │
                                                                                   ▼
                                                                    Phase 8  hardening
                                                                                   │
                                          ┌────────────────────────────────────────┴──────────┐
                                          ▼                                                   ▼
                     Phase 9  quality foundations                           Phase 11 sandbox foundation
                                          │                                                   │
                                          ▼                                                   ▼
                     Phase 10 quality improvement                           Phase 12 sandbox resilience
                                          └──────────────────────┬────────────────────────────┘
                                                                 ▼
                                               `/gamedebug release` publication gate
                                                                 │
                 Phase 13 minimal editor (parallel, preserves behaviour)          │
                                                                 ▼
                  Phase 14 capability registry ──> Phase 15 GameSpec/planner
                                                                 │
                                                                 ▼
                                             Phase 16 runtime kernel/contracts
                                                ┌────────────────┼───────────────┐
                                                ▼                ▼               ▼
                                Phase 17 render/assets   Phase 18 physics   Phase 19 media
                                                └────────────────┼───────────────┘
                                                                 ▼
                                         Phase 20 gameplay/nav/AI + Phase 22 graphs/plugins
                                                                 │
                                                                 ▼
                                      Phase 21 terrain/worlds → Phase 23 save/network/export
                                                                 │
                                                                 ▼
                                             Phase 24 production proof
```

Phase 4 can run alongside 2/3 once Phase 0 lands, because it adds no logic. Phase 6 needs
Phase 5 (things must render) and Phase 3 (the HUD must exist) and the ENG-168 decision.
Phase 9 may begin after Phase 8's deterministic fixtures exist. Phase 11 may proceed in
parallel with Phase 9, but Phase 10 cannot claim runtime quality without Phase 11's bounded
execution contract. Phase 12 and the release-mode quality floor both feed the final publication
gate; neither may be replaced by a model-written review.
Phase 13 may simplify presentation in parallel but may not change engine ownership or remove
routes. Phase 14 is the breadth gate: Phases 17–23 may prototype behind ADRs, but no new public
subsystem is complete until it registers and is retrievable/composable. Phase 16 precedes
runtime-heavy work; Phase 18 precedes character/vehicle gameplay; Phase 19 precedes polished
combat; Phase 17/16 precede large-world streaming; runtime identity/save precede networking.

---

## 9. Definition of done per task (copy this into your final message)

```
[ ] The task's acceptance line is provable, and I can name the test that shows it
[ ] Invariants touched: INV-___, INV-___   (enforced in code, not in a prompt)
[ ] Unit tests for the logic; fixture tests for anything that parses
[ ] cargo fmt · cargo clippy -D warnings · cargo test --workspace   all clean
[ ] tests/architecture.rs still passes (no unplanned crate edge)
[ ] IPC bindings regenerated if the command surface changed
[ ] tsc --noEmit + vite build clean
[ ] tracing spans added; errors typed with an actionable hint
[ ] `/gamedebug` stage ids/statuses and report schema remain backward-compatible, or the schema version and migration fixture changed together
[ ] Generated-game work names its rubric/corpus case and stores machine-readable evidence; no model self-score is counted as proof
[ ] Runtime work proves default-deny capabilities and every applicable resource budget with an adversarial test
[ ] Diagnostic and sandbox runs leave authored files byte-identical unless an explicitly approved `--fix` transaction ran
[ ] Editor work preserves the single Outliner/Inspector/drawer/command registries; no duplicated panel or catalogue
[ ] At 1440×900 the main toolbar does not wrap, the viewport remains dominant and advanced controls use progressive disclosure
[ ] A new public subsystem reports all seven truth dimensions and has a real capability-registry entry
[ ] Any integrated dependency has an accepted ADR, pinned version, licence/provenance record and platform fallback/unsupported state
[ ] docs updated: PROGRESS.md row, this file's checkbox + §12 log
[ ] No new dependency, screen, option or seam that was not asked for
```

---

## 10. Honest verification script

Run this by hand before ticking anything in Phases 3–24.

```
[ ] Open a NON-game folder     → Engine chrome, empty grid, empty Content Browser
[ ] New Game                   → Main + HUD + level_01 on disk, manifest correct
[ ] Ask the AI to build a level→ entities appear live, one undo step reverses all of it
[ ] Hand-edit what it built    → gizmo drag, Details edit, rename, reparent — all persist
[ ] Open the HUD file          → change button text, colour, anchor — saved to hud.json
[ ] Double-click main → Play   → the game runs: input, physics, HUD, pause, level travel
[ ] Stop                       → the authored scene is byte-identical to before Play
[ ] Kill the app mid-edit      → reopen recovers the session from the journal
[ ] 1 000-entity scene         → ≥55 fps, panels stay responsive
[ ] Release build w/ unknown-licence asset → BLOCKED, with the asset named
[ ] `/gamedebug quick`       → same ordered stage ids and same findings on two unchanged runs
[ ] `/gamedebug full`        → fixed-seed play trace, screenshot and authored pre/post hash saved
[ ] `/gamedebug release`     → quality floors + content gates + sandbox hostile corpus all block on failure
[ ] Corrupt generated game   → report names file/entity, evidence, reproduction and actionable repair
[ ] Attempt forbidden host call → typed sandbox denial; no network/process/secret access and no authored mutation
[ ] Infinite loop/output flood → the exact CPU/step/output budget stops it and the next run starts cleanly
[ ] 1440×900 default editor   → one-line toolbar, dominant viewport, one Inspector, bottom drawer collapsed
[ ] Five common editor tasks → no documentation, no toolbar wrap, no hidden permission/error state
[ ] Capability retrieval     → relevant bounded cards, no whole-registry prompt and no hallucinated id accepted
[ ] New subsystem claim      → documented + implemented + tested + editor/AI accessible + runtime/platform evidence shown separately
```

**Run conditions:** use a clean copy of `tests/fixtures/engine/warehouse_game/`; record app
build hash, OS/GPU, viewport pixel size and whether the provider is deterministic or live.
Do not reuse a project with an existing autosave/journal. For the performance row, warm the
asset cache once, then record a 30-second sample; a single instantaneous fps label is not
evidence.

**Evidence bundle:** save the pre/post authored-tree hashes, journal export, final viewport
PNG, scripted playtest report, content-gate report, accessibility report, performance JSON
and build-ledger rows under the test artefact directory. A failure keeps the artefacts and
the corresponding box open. Never hand-edit an artefact to make the assertion pass.
Game-debug evidence additionally includes the canonical JSON/Markdown pair, rubric version,
corpus id, deterministic seed, input trace, sandbox backend/capabilities, budget counters and
the hash of every authored input. `latest.json` must point to that immutable run rather than
contain a mutable second rendering of it.

**Automated companion gates:**

```
[ ] Rust format + clippy + workspace tests + architecture guard
[ ] IPC export is byte-fresh; TypeScript typecheck + production build
[ ] Engine unit/fixture suites: document, action, HUD, material, prefab, script, gates
[ ] UI runtime suites: play, script VM, HUD interaction, observation and editor keyboard
[ ] Offline autonomy golden transcript (no network)
[ ] axe engine-state matrix (zero serious/critical)
[ ] headless perf fixture + browser/GPU reference run
[ ] Windows + Web host build/smoke lanes, with ledger assertions
[ ] Quality corpus: per-case floors, aggregate floor, baseline delta and mutation tests
[ ] Sandbox corpus: capability denial, traversal/symlink, network/process/secret, fork/spawn,
    infinite loop, memory/output flood, crash recovery and repeated-run isolation
[ ] Report schema golden + stable finding-code inventory + authored-tree immutability test
```

---

## 11. Known risks

| Risk | Why it matters | Mitigation |
|---|---|---|
| Cross-boundary observation can hang or answer the wrong request | Rust owns the model loop while ADR-0028 puts rendering in the webview | One-shot request ids, active-pane routing, strict timeout, late/duplicate rejection and bounded payloads |
| Editor chrome grows faster than capability | The present toolbar already exposes transport, gizmos, snap, shading, cameras, AI, weather and drawers at once | Phase 13 fixed shell, progressive disclosure and visible-control/viewport/task budgets; no new permanent toolbar control |
| Docking recreates complexity before the default workflow is calm | Floating windows, saved layouts and focus restoration multiply UI states | ADR-0034 must first prove the fixed/preset shell; optional docking is later expert functionality, not Phase 13's foundation |
| Model providers differ on tool calling | The tag protocol is the only universal path | ENG-113 keeps tags working; ENG-114 upgrades where supported |
| Context budget blowout | The engine context competes with the rest of the turn | ENG-191, measured against `docs/token-engine/baseline.md` |
| Webview runtime and Rust compiler drift | Script bytecode, host calls and runtime reports cross a language boundary | Shared committed fixture + ABI-by-name tests + prompt inventory guard (ADR-0030) |
| Asset conversion silently changes scale/handedness/materials | Imported content can look plausible while being physically wrong | ENG-124 source fixtures, explicit import report and deterministic reimport recipe |
| GPU CI is unavailable or unrepresentative | A headless 1k-entity Rust test cannot prove 55 fps | Keep headless and GPU lanes separate; publish hardware/viewport protocol and never tick INV-077 from CPU evidence |
| The generating model grades its own output generously | A fluent explanation can conceal an unplayable or incomplete game | Engine-owned rubric, deterministic assertions and independent evidence; model commentary is never a score input |
| The quality suite overfits a few showcase games | Scores rise while novel mechanics regress | Versioned diverse corpus, hidden mutation/property cases, per-category floors and periodic human calibration |
| Runtime observations are nondeterministic | Timing, random seeds and GPU variance make failures hard to reproduce | Fixed clock/input/seed for gates, variance envelope for GPU evidence and complete replay metadata |
| A web worker is mistaken for a security sandbox | Workers isolate responsiveness, not necessarily host authority or browser-origin capabilities | Explicit backend capability matrix; default-deny broker; OS containment for untrusted native execution; honest `unsupported` status where unavailable |
| The sandbox broker becomes a confused deputy | A narrow guest call can still make the host read/write an attacker-chosen target | Typed operations, canonical project-relative paths, symlink/TOCTOU tests, capability-scoped handles and audit log |
| Resource exhaustion survives a stopped run | A killed guest can leak processes, files, handles or poisoned shared state | Per-run disposable state, supervisor cleanup, orphan detection and a clean-run-after-fault acceptance test |
| Capability registry becomes another stale catalogue | AI retrieves confident metadata that no longer matches code | Generate structural facts from Rust owners, architecture completeness test, maturity evidence and stable ids |
| “Unreal-class” becomes an endless parity claim | Hundreds of labels can crowd the UI and hide primitive runtimes | Dependency phases, seven-dimensional truth vocabulary, representative game goldens and production budgets; never claim feature parity from breadth |
| Building low-level engines wastes years | Physics/audio/navigation/import/render utilities are specialised and licence-sensitive | Explicit BUILD/INTEGRATE/WRAP/ADAPT ADR per subsystem, measured prototype, maintenance/platform/licence review |
| Scope expands beyond a usable engine | `engine plan.md` and this audit span many advanced systems | Capability registry and representative templates define demand; P0/P1 foundations precede P2/P3 breadth and unsupported remains honest |

---

## 12. Progress log

Append one row per session. Never delete a row.

| Date | Agent | Tickets | What actually shipped | Evidence |
|---|---|---|---|---|
| 2026-09-01 | codex | ENG-241/243 done; ENG-242/245/246/250/251/253 advanced | Started Phase 13. Accepted ADR-0034 and simplified the live Engine shell: the default app bar now keeps scene/save, Play/Stop, Add, AI and More; transform/snap/shading/camera/Show/options live in one viewport context strip; Scene/HUD live in a narrow mode rail; AI capabilities and advanced commands remain available through focused menus using their existing handlers. Added explicit 1200/900 px degradation and source guards against toolbar/panel regression. | `npm run build`; full UI suite 62/62 including 3 new shell tests; browser preview boot/DOM check. Project-backed Engine visual capture remains open because the browser-only preview has no Tauri project IPC. |
| 2026-09-02 | codex | ENG-244 done; ENG-247 advanced | Continued Phase 13: Play/Pause and Stop remain direct while Restart, Step, speed, Game View, Eject/Possess, break-on-error and live metrics appear only in a Play options surface. Consolidated Content and the real Output Log into one bottom drawer with Problems, AI Activity, Game Debug and Build Targets tabs; collapsed is the default, `Ctrl+J` toggles it, tab/open state persists per project, and new errors raise a populated Problems tab without moving focus. | `npm run build`; full UI suite 64/64 including Play-options and shared-drawer guards. ENG-247 remains partial until resizable height persistence and automatic game-debug report raising ship. |
| 2026-09-01 | codex | ENG-240…399 specified; implementation not started | Incorporated the expanded Unreal-class audit as a truthful capability matrix and dependency-ordered Phases 13–24. Added the minimal editor reset: one calm toolbar, mode rail/context panel, dominant viewport, one Inspector and a shared bottom drawer; progressive disclosure replaces the current everything-toolbar. Added seven separate maturity dimensions, exact handoff state, registry-first AI composition, subsystem BUILD/INTEGRATE/WRAP/ADAPT decision gates and measurable UI/runtime/production acceptance. | Read the supplied audit; inspected the existing `EngineView` toolbar/layout and current engine modules/docs; Markdown/ticket/status checks. No checkbox was promoted from prose. |
| 2026-09-01 | codex | ENG-200 done; ENG-201/206/207/208 advanced | Expanded the plan with two quality and two sandbox phases, accepted ADR-0032, and shipped the first real `/gamedebug` slice: fixed nine-stage Rust graph, manifest/scene/HUD/input/material/shader/asset/licence/script checks, authored hashes, stable AI-ready findings, offline command/composer integration, immutable JSON/Markdown run reports and latest pointer. Full/release runtime stages remain explicitly unsupported/incomplete; no prose or placeholder was promoted to pass. | `game_debug` engine tests 4/4; app parser/report-store tests 2/2; full workspace tests; workspace/all-target clippy `-D warnings`; Rust format/diff checks; TypeScript + Vite production build; authored-tree immutability assertion. |
| 2026-09-01 | codex | ENG-115, 126, 136, 142, 144, 147, 152, 165, 166; ENG-107/149 advanced | **Closure wave.** Added bounded project/asset/console/play queries; deterministic room/corridor actions; safe HUD hierarchy keyboard moves; schema-owned multi-edit/default reset; complete orthographic/view-mode/screen-percentage/maximise controls; project-scoped quick-open; collider/bounds truth from Play's resolver; and resource-sharing selected-camera PiP. Incremental scene patches preserve untouched Three resources, and the shared typed console now opens exact source lines; only browser projection timing and restart persistence remain on those two tickets. | Full workspace gates green: `cargo fmt --all -- --check`; `cargo clippy --workspace --all-targets -- -D warnings`; `cargo test --workspace`; 59/59 UI tests; production TypeScript/Vite build. |
| 2026-09-01 | codex | ENG-185–188, 191, 197, 198; ENG-195/199 headless slices | **Autonomy closure + hardening.** Added true six-round plan/act/observe/repair control, repeated-patch and structural-fault exits, typed one-shot camera capture with PNG/IHDR/timeout bounds, fixed-step scripted playtest reports with authored hashes, four repair fixtures, and fixed 1,500-token dynamic context evidence. Added a machine-readable 1k headless perf gate including event projection, axe’s 32-state Engine matrix, canonical hashed release fixtures, offline Debug/Release preflight, ADR-0031 and authoritative doc reconciliation. Reference-GPU fps and launched host artefact/DB-ledger evidence remain open under ENG-195/199. | `engine_autonomy_golden::warehouse_key_door_repairs_and_verifies`; observation Rust tests; 39 UI runtime/capture tests; 3 repair fixtures; `perf_budget` 5/5; axe 32-state matrix; `golden_release` 3/3; TypeScript/Vite build. |
| 2026-09-01 | codex | Plan completion/reconciliation | Completed every remaining ticket as an implementation contract: corrected false-complete markers, reconciled the interrupted Phase 7 observation/playtest work, added dependency-ordered closure cards with authoritative seams/tests/failure behaviour, retired stale INV-078 hardening work, split headless vs. GPU gates, and specified the offline + host-toolchain golden E2E lanes. No implementation ticket was promoted to done without its named evidence. | Markdown structure check; checkbox/status audit; cross-check against current `engine/observation.rs`, `chat.rs`, `EngineViewport.tsx`, `playRuntime.ts`, module contracts, invariants and ADR-0028/0030. |
| 2026-09-01 | codex | ENG-104, 170–175, 177–180, 196 (172/173/177/179 partial) | **Playable runtime + recovery.** Validated/scaffolded `bhippi-input@1`; isolated runtime world with manifest gravity, component-driven controller, collisions/sensors, named input, live HUD bindings/actions, persistent level travel, camera possess/eject and full transport/diagnostics. Transaction snapshots survive a process restart and replay dirty without touching the authored file. Added INV-081 and replaced the stale Bevy M15 contract per ADR-0028. | `input` unit tests; `play-runtime.test.mjs` (named input, pause/step/restart, authored-state immutability); `engine_sessions::crash_snapshot_is_offered_replayed_and_cleared_only_after_save`; regenerated IPC; Rust/UI workspace gates. |
| 2026-09-01 | claude | — | Audited the chat↔engine seam end to end; wrote this plan (F1–F9 findings, phases ENG-100…199) | this file |
| 2026-09-01 | claude | ENG-100…103, 105, 106, 109 (+107/108 partial) | **Phase 0 — one write path.** `EngineSessions` (open documents, one undo stack, dirty flag, revision, disk stamp, live interactions) behind a process-wide store both the IPC commands and the chat bridge use, so F1's two write paths are now one. 16 new commands (open/reload/close/save/save_all/undo/redo/begin+record+commit+cancel interaction/set_selection/history/weather_presets/templates/play_world). `bhippi-db::EngineRepo` + migration `0011` finally write `engine_journal` (F2), storing ops, **inverse**, touched and actor per transaction. `EngineSceneDocument.ts` reduced to types + one decode helper — scene creation, ULID generation, weather, duplication, kind inference and scene merging all moved to Rust (F3); `EngineView.tsx` rewritten as a controlled view that dispatches actions and renders `EngineSceneState`. New engine modules `weather.rs` (presets + a test binding them to the schema enum) and `compose.rs` (deterministic Main+level+HUD play composition, replacing the TS merge that emitted parser-rejecting ids). New `Op::SetSettings` + `SetWeather`/`SetSceneSettings` actions so scene-level edits are transacted too. | 8 session acceptance tests (agent edit preserves unsaved user work; undo spans both actors; a drag is one undo step; cancel leaves no trace; dirty close refuses; save round-trips the strict parser; outside rewrite reported as conflict; journal facts captured), 2 journal tests, 1 new architecture guard (`the_webview_never_writes_a_scene_or_computes_engine_state`), 5 weather/compose + 4 action tests. Workspace green (63 engine, 87 app lib, 104 core, all suites 0 failed), clippy `--workspace` clean, `cargo fmt --check` clean, `tsc --noEmit` + `vite build` clean, bindings regenerated. |
| 2026-09-01 | claude | ENG-111…113, 118, 119 (110/115/116/117 partial · 114 blocked) | **Phase 1 — the AI ↔ engine bridge.** `EngineAction` grew from 11 to 23 scene-scoped verbs (`translate`, `look_at`, `set_component_property` by dotted path, `set_tags`, `set_visible`, `set_locked`, `set_mesh`, `set_material`, `attach_script`, `group_entities`, `align_entities`, `distribute_entities`) with new `Op::SetTags` and a `Visibility` component; the engine does the geometry (look-at quaternion via Shepperd's method, group centroid, distribute ordering) so the model never does. `EngineActionBatch` makes a batch **one** transaction, one journal row, one undo step, all-or-nothing, with a typed `EngineActionOutcome` per action carrying the failing index, the engine's hint and that component's real `schema::excerpt`. `engine/bridge.rs` scans `<engine_action>` / `<engine_batch>` / `<engine_query>` **out of the live stream** — tags split across deltas are reassembled, protocol text never reaches the visible answer, a truncated call is dropped rather than half-applied — and `chat.rs` now runs a bounded (3-round) read→act→verify loop that answers queries and feeds rejections back with the schema. `engine/query_bridge.rs` answers 12 query kinds out of the SEC 7.4 `SceneQueries` API that had been built and unused. ENG-116: `EnginePermissionMode` (ask/auto/autonomous) in config + IPC + an Engine-toolbar picker + a plan card gated on the existing permission channel, defaulting to auto-except-deletes. `prompts/chat-engine.md` v5 rewritten around the protocol, including an explicit "what you cannot do yet" list. | The golden test caught a real bug that reasoning alone had missed: `apply_batch` originally validated on a scratch copy then re-lowered against the live document, but `spawn` mints a fresh ULID on every lowering, so the second pass created entities that the batch's own later actions — resolved against the first pass — no longer referred to. The single-pass ops are the ones that must commit. Separately, ENG-114 is blocked by architecture rather than effort: `CompletionRequest` has no `tools` field, `Delta` has no tool-call variant, and the CLI adapters run their own tool loops as subprocesses, so native tool calling needs an ADR extending the provider contract before any code. | Phase 2 (`ENG-120…128`): `bhippi-material@1` + `bhippi-shader@1` formats and validation, asset writes on the transaction path with a file-deleting inverse, licence sidecars feeding the INV-074 Release gate, the import pipeline, seeded procedural helpers, provenance, and the content gates — which is also what unblocks the asset-creating half of ENG-110. Run `cargo test --workspace` (366 green) + `cargo clippy --workspace` + `cargo fmt --check` + `tsc --noEmit` + `npm run build` before finishing. Note `clippy --all-targets` trips on pre-existing `expect()` in `bhippi-engine-build` tests; the documented gate is lib targets. |
| 2026-09-01 | claude | ENG-120…123, 125…128 (124 partial) | **Phase 2 — real content generation.** New engine modules: `material.rs` (`bhippi-material@1` + `bhippi-shader@1`, refusing out-of-range values rather than clamping), `prefab.rs` (`bhippi-prefab@1` — capture, instantiate with fresh ULIDs, override-aware propagation that never moves an instance), `procedural.rs` (pinned SplitMix64 + grid/scatter/ring/perimeter/stack, seeded and capped) and `gates.rs` (`check_project` + `check_assets`, typed blocker/warning findings). New app module `engine/content.rs`: `ContentAction` (`create_material`, `create_shader`, `create_prefab`, `set_asset_license`) producing a `FileChange` that carries prior bytes, so the inverse is exact; content steps ride inside the same batch as scene actions and the session keeps a file ledger keyed by transaction id, making "create a material and put it on the floor" one Ctrl+Z. Five new placement verbs make the procedural helpers callable. Every spawn is stamped with `Provenance` (actor, txn, timestamp) in `commit_ops`. The gates are wired where they block: `bhippi-engine-build::prepare` fails on any blocker, and `engine_check_content` exposes the report. `prompts/chat-engine.md` v6. | Three real defects surfaced. (1) The scaffold had been writing `lit_pbr.mat.json` / `lit_pbr.shader.json` as hand-written constants wearing the `bhippi-material@1` / `bhippi-shader@1` markers **while predating the formats** — the first parser to exist would have rejected a new game's own starter material; both now come from the real types plus a real `lit_pbr.wgsl`. (2) The scaffold wrote no `.meta.json` sidecars, so a brand-new project could not produce a Release build: INV-074 blocked it on its own starter content. (3) `bhippi-engine-build::collect_tree` matched scenes on `extension == "bscn"` but the files are `*.bscn.json` (extension `json`), so **no scene had ever been collected** and the structural pass in `collect` had been iterating an empty list since the crate was written. Also worth recording: `EngineSessions::apply_batch` originally validated on a scratch copy then re-lowered against the live document, which is wrong because `spawn` mints a fresh ULID per lowering — a golden test caught it and the single-pass ops are now what commit. | Phase 3 (`ENG-130…139`): the `bhippi-hud@1` widget format, its schema registry, the 2D canvas editor, the Details panel and the runtime renderer — the owner's explicit "a HUD file with all the options, editable by hand". Note `ui/` currently fails `tsc` with 27 errors from an **unfinished plugins feature** another agent left mid-edit (`Sidebar.tsx`, `App.tsx`, `Chat.tsx`, `api.ts` wrappers calling `commands.listPlugins` etc., whose `commands.rs` functions were never registered in `lib.rs`); none touch `ui/src/engine/`. That needs finishing or reverting before the UI builds again. Run `cargo test --workspace` (420 green, 53 suites) + `cargo clippy --workspace` (8 remaining warnings, all in that same unregistered plugin block) + `cargo fmt --check`. |
| 2026-09-01 | claude | ENG-130…137, 139 (138 partial) | **Phase 3 — the HUD system**, plus a build fix. New `bhippi-engine` modules `hud.rs` (`bhippi-hud@1`: canvas, anchored rects, style, bindings, a closed click-action list, twelve widget kinds with typed field schemas) and `hud_action.rs` (thirteen edit verbs plus `resolve_rect`, the anchor maths the canvas editor and the runtime overlay share). New app module `engine/hud_session.rs`: open HUD documents with a snapshot undo stack, a widget catalog for the Add menu, and widget views whose rects arrive pre-resolved so the webview computes nothing. Nine `hud_*` IPC commands, a `HudChanged` event, and `EngineHudEditor.tsx` — a Scene/HUD tab in the Engine pane with a widget tree, a canvas preview and a Details form generated from the engine's schema, committing a whole form as one undo step. ENG-139: the scaffold now writes `assets/ui/hud_main.hud.json`, the manifest and Main point at it, `compose_play` stopped merging HUD entities into the 3D world, and `engine_play_world` returns the HUD document plus resolved widgets so Play draws the overlay from the *live session*. | The old HUD was entities carrying `UiDocument { layout: "health" }` — a magic string with no fields behind it, which is precisely why a button's text could not be changed: there was nowhere for the text to live. Also fixed here: the **plugins feature another agent left half-finished** was breaking `tsc` and the whole UI build (27 errors). Its Rust side was complete but never registered in `lib.rs`, so `PluginMetadata` never reached `ipc.ts`; `Sidebar.tsx` had a clobbered `useEffect` opener, a duplicate local `PluginMetadata`, missing icon and `api` imports, a `"plugins"` value absent from the `Screen` union and a dead `onOpenPlugins` prop; `Chat.tsx` had its own copy of `PluginMetadata` typed `window?: PluginWindow` where the wire type is `window: PluginWindow \| null` — the exact drift that hand-copied generated types produce. Registered, wired and unified; the app builds and runs again. | Phase 4 (`ENG-140…152`, Unreal-grade editor UX: docking, Outliner folders, schema-driven Details for *scenes*, Content Browser tiles, viewport toolbar, marquee select, command palette, output log) or Phase 5 (rendering truth — the renderer still does not draw materials or imported meshes, which is the last thing standing between "the AI generated it" and "I can see it"). Run `cargo test --workspace` (452 green, 53 suites) + `cargo clippy --workspace` (clean) + `cargo fmt --check` + `tsc --noEmit` + `npm run build` + `cargo build -p bhippi-app --bin bhippi-desktop`. |
| 2026-09-01 | claude | ENG-141, 142, 146, 147, 149, 150 (143/144/145/148/151/152 partial · 140 deferred) | **Phase 4 — Unreal-grade editor UX.** `engine_component_schema` and `engine_list_assets` expose the engine's component registry and asset index over IPC, and the Details panel is now generated entirely from them — accordions by category, a control per `FieldKind`, asset pickers filtered by `AssetKind`, property search, Add/Remove Component from the live registry. The Outliner became a real tree: expand/collapse, drag-to-reparent, Ctrl/Shift multi-select, per-row visibility and lock, type icons, search and filter chips including **AI-made** (which works because ENG-127 stamps provenance). New `EngineCommandPalette` (`Ctrl+Shift+P`, fed the same handlers the toolbar calls) and `EngineOutputLog` (the transaction journal plus local notices, so "what did the agent change?" survives a restart). Change toasts name the actor and offer one-click Undo. Viewport gained a Show-flags menu and a camera-speed slider. | **ENG-146 closed F9, the last structural defect from the original audit:** the viewport added every object flat to the scene group under a comment admitting the hierarchy was "logical, not transform-accumulated", so moving a parent left its children behind — which is why prefabs, rigs and grouped level pieces could not work. Objects are now parented to each other, `Transform` is local as the stable-path addressing (`scene:/Parent/Child`) always implied, and a gizmo drag writes the local transform so a dragged child takes its own children with it. ENG-140 (docking) was deliberately not started: every other Phase 4 ticket makes something possible that was not, while docking makes something movable, and the renderer still cannot draw a material. | Phase 5 (`ENG-160…168`, rendering truth): GLTF loading, PBR material application, lighting parity, sky/IBL, editor visuals, the 55 fps INV-077 gate, and the **ENG-168 decision** — build the Bevy viewport behind the JSON-RPC protocol that already exists, or amend ADR-0020 to make the webview viewport the shipping renderer. That decision has been "next" since 2026-08-29 and now blocks Phase 6's physics. Run `cargo test --workspace` (463 green, 53 suites) + `cargo clippy --workspace` (clean) + `cargo fmt --check` + `tsc --noEmit` + `npm run build` + `cargo build -p bhippi-app --bin bhippi-desktop`. |
| 2026-09-01 | claude | ENG-160…164, 168 (165/167 partial · 166 not started) | **Phase 5 — rendering truth, and the decision that was blocking Phase 6.** Wrote **ADR-0028**: the webview is the shipping viewport renderer, ADR-0020's Bevy child-process model is withdrawn. Removed `bevy.rs` (a 13-line stub that could not open a window), the `bevy` feature, the dependency and the binary target; `protocol.rs` stays as a documented unused design with the reversal conditions named. **Closed F8**, the last rendering defect: new `bhippi-engine::mesh` gives `MeshRenderer.mesh` exactly three legal forms (empty / `builtin:<name>` / `asset:<ulid>`) and the schema rejects the bare `"cube"` the old TypeScript wrote; new `engine_render_manifest` resolves every mesh and material the open scene references — parsing `.mat.json`, resolving `asset:` textures to files, filling defaults — and new `ui/src/engine/renderResources.ts` builds shared cached geometries, textures and `MeshStandardMaterial`s from it, loading GLB/GLTF and normalising imported models to unit scale. Lights now honour colour, intensity, range and `outer_angle` (spots were drawn as bare points). Weather presets drive ambient and key-light intensity, not just the backdrop. A reference that does not resolve draws as a loud magenta wireframe and is logged. New `perf_budget.rs` measures the engine-side half of INV-077 in CI. | The viewport used to *guess*: an entity named "floor" got one grey box, a `.glb` got a different grey box, and `grep albedo EngineViewport.tsx` returned zero — material maps were stored and never applied. That is why "the AI generated it" and "I can see it" were different claims. The colour-space split matters and is easy to get wrong: albedo and emissive are sRGB, normal/roughness/metallic/AO are data and must stay linear. On ENG-168: the cost of the Bevy path was never the renderer, it was child-process lifecycle, Windows window embedding, DPI and resize sync, input forwarding and a JSON-RPC transport on the hot path — weeks of work whose deliverable is the same picture in the same rectangle. | Phase 6 (`ENG-170…180`, play mode that actually plays): Rust play composition already exists, so next are snapshot/restore, physics, the character controller, camera possession, the input map, the Rhai script runtime, the live HUD bindings and level travel — all of which now have a settled home (in the webview, per ADR-0028). Note `docs/06-INVARIANTS.md` still lists INV-072 and INV-078 as active; ADR-0028 retires them and the invariants table needs the edit. Run `cargo test --workspace` (469 green, 53 suites) + `cargo clippy --workspace` (clean) + `cargo fmt --check` + `tsc --noEmit` + `npm run build`. |
| 2026-09-01 | claude | ENG-172, 173, 176, 177, 179, 189, 190, 192 | **Phase 6 closed and Phase 7's safety half.** **ADR-0030** settles the ENG-176 blocker by splitting scripting: `bhippi-engine::script` (lexer, parser, compiler → bytecode with per-instruction line spans, 43 host functions, subset violations rejected *by name*) and `ui/src/engine/scriptVm.ts` (a ~400-line stack VM with a 200 000-step budget, a depth cap, no `eval`, no `Function`). New **INV-082**, enforced by a grep in `tests/architecture.rs`. The conservative AABB solver is replaced by real cuboid/sphere/capsule/heightfield colliders resolved against **oriented** boxes, which is what makes `max_slope` and `step_height` real. `create_script` compiles before it writes. New `bhippi-engine::capability` — seven capabilities × allow/ask/deny in `[agent]` in `Bhippi.game.toml` — gated at `apply_batch_in_workspace` and never applied to the user. Scene leases plus an optimistic revision check for two agents or an agent and the user. "Undo AI change" replays a journalled batch's inverse as a new, itself-undoable user transaction. | Ticking ENG-176 either way would have been a lie: Rhai is a Rust crate and ADR-0028 put the runtime in the webview, so the choice was IPC on the frame path or `eval` in the pane. The ADR names a third option and the cost of the other two. Two drift guards matter more than the features: `script_fixture.rs` fails in Rust if the compiler stops emitting the program `ui/tests/play-runtime.test.mjs` executes, and a prompt test fails if `chat-engine.md` stops listing a host function the compiler accepts. Also found and fixed: the repo's own `cargo clippy --workspace --all-targets -- -D warnings` gate was red across untouched crates, because `expect_used = "deny"` fires in every test module. | Phase 7's autonomy half is one coherent piece of work, not five: ENG-186's `engine_screenshot` and ENG-187's scripted-input playtest are what ENG-185's loop driver and ENG-188's repair round are made of. Start with the capture channel. ENG-191 (context budgeting against `docs/token-engine/baseline.md`) is independent and can go first if preferred. Phase 8 remains: ENG-195 (CI perf budgets), 197 (axe), 198 (docs), 199 (golden end-to-end). |
