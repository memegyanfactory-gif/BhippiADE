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
(165/166/167 remainders) · Phase 6 complete · Phase 7 in progress (observation/playtest
bridge present but not yet release-proven) · Phase 8 partial · ticket range
`ENG-100…ENG-199`.

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

`ENG-100…ENG-199` is reserved for this plan. Where a task completes or supersedes an existing
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

- [ ] **ENG-140** NOT STARTED (claude, 2026-09-01 — deliberately deferred, see below) — Docking system: panels are dockable / tabbed / floating / resizable,
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

#### Final Git publication gate

After — and only after — every required ticket and the Phase 8 acceptance evidence above are
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
| Editor UI | `ui/src/engine/*` (existing surfaces plus planned `EngineDock`; names must follow the current component layout rather than create duplicate Outliner/Details/Content Browser implementations) |
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
```

Phase 4 can run alongside 2/3 once Phase 0 lands, because it adds no logic. Phase 6 needs
Phase 5 (things must render) and Phase 3 (the HUD must exist) and the ENG-168 decision.

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
[ ] docs updated: PROGRESS.md row, this file's checkbox + §12 log
[ ] No new dependency, screen, option or seam that was not asked for
```

---

## 10. Honest verification script

Run this by hand before ticking anything in Phases 3–6.

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
```

---

## 11. Known risks

| Risk | Why it matters | Mitigation |
|---|---|---|
| Cross-boundary observation can hang or answer the wrong request | Rust owns the model loop while ADR-0028 puts rendering in the webview | One-shot request ids, active-pane routing, strict timeout, late/duplicate rejection and bounded payloads |
| Docking can destabilise otherwise working editor panels | Floating windows, saved layouts and focus restoration multiply UI states | Versioned split-tree model, corrupt-layout recovery, minimum sizes, keyboard docking and preset fixtures before visual polish |
| Model providers differ on tool calling | The tag protocol is the only universal path | ENG-113 keeps tags working; ENG-114 upgrades where supported |
| Context budget blowout | The engine context competes with the rest of the turn | ENG-191, measured against `docs/token-engine/baseline.md` |
| Webview runtime and Rust compiler drift | Script bytecode, host calls and runtime reports cross a language boundary | Shared committed fixture + ABI-by-name tests + prompt inventory guard (ADR-0030) |
| Asset conversion silently changes scale/handedness/materials | Imported content can look plausible while being physically wrong | ENG-124 source fixtures, explicit import report and deterministic reimport recipe |
| GPU CI is unavailable or unrepresentative | A headless 1k-entity Rust test cannot prove 55 fps | Keep headless and GPU lanes separate; publish hardware/viewport protocol and never tick INV-077 from CPU evidence |
| Scope creep into a full Unreal clone | `engine plan.md` lists 116 systems | This plan deliberately covers only what the owner's goal needs; anything else needs a new ADR |

---

## 12. Progress log

Append one row per session. Never delete a row.

| Date | Agent | Tickets | What actually shipped | Evidence |
|---|---|---|---|---|
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
