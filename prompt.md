# Bhippi Engine + Workbench — living prompt

**Doc:** `prompt.md`  
**Audience:** every AI agent working in this repo  
**Status:** living. Read this **before** Engine / Browser / Usage / Workbench work. Update the Done / Remaining tables when you ship something.  
**Date opened:** 2026-08-30  
**Owner request:** make the in-app browser open real websites, show Grok weekly usage remaining, let the Editor/Browser/Engine side panel expand well past 50% (chat shrinks with it), restyle the effort slider like the Cursor screenshot, speed the app up, and turn the Engine into an Unreal-style pipeline the AI can actually generate into.

If this file disagrees with code, **the code wins** — then you fix this file in the same change (`docs/07-AGENT-GUIDE.md`).

Authority still holds: spec → invariants → architecture → module contracts → ADRs → this file → code comments. This file does **not** override INV-*, R1–R12, or ADR-0020’s crate edges.

---

## 1. What the product is (one paragraph)

Bhippi is a Rust + Tauri ADE. Chat is the left column. The right **workbench** has three pills: **Editor · Browser · Engine**. Engine is a game-engine workbench in the spirit of Unreal: a 3D viewport, Outliner, Details, Content Drawer, Play, and a machine-readable mind map so any chat provider (Claude, Codex, Grok, OpenCode, local) can create a **Main** map, **HUD**, and **Levels**, wire them, and have Play run the full game.

---

## 2. Non-negotiables (do not “fix” these away)

- Technology/AI topics only for the **research/publishing** pipeline. The Engine is a workbench capability (ADR-0020) — it may build games.
- No `unwrap()` / `expect()` outside tests.
- No SQL outside `bhippi-db`.
- No business logic in TypeScript that Rust should own (R3). Viewport rendering is currently Three.js in the webview as the P1 stand-in; Bevy child-process viewport is still the ADR end-state.
- No hand-edited `ui/src/lib/ipc.ts` — regenerate with `cargo run -p bhippi-app --bin export-bindings`.
- No prompt strings in code. Engine AI doctrine lives in `prompts/chat-engine.md`.
- Gates block, they never warn.
- Do **not** modify `C:\Work\VSCode\Bhippi` (unrelated static site).
- Do not invent Grok/OpenCode weekly numbers. Probe the vendor; if the vendor does not report a window, the UI says **Not reported**.

---

## 3. Mind map — how the Engine is supposed to work

This is the contract the AI and the UI share. If you change it, update `prompts/chat-engine.md`, `.bhippi/engine/engine-map.json`, and this section together.

```
Project (folder on disk)
└─ Bhippi.game.toml          ← presence of this file = “this is a game”
   [game]
     default_scene = "assets/scenes/main.bscn.json"   # the Persistent / Main map
     hud_scene     = "assets/scenes/hud.bscn.json"
     levels        = ["assets/scenes/level_01.bscn.json", ...]
   assets/
     scenes/
       main.bscn.json        # GameMode + streaming list + camera + player spawn
       hud.bscn.json         # HUD widgets (UiDocument). Editable independently.
       level_01.bscn.json    # a playable map
       level_02.bscn.json    # …
     models/                 # .glb / .gltf / .obj imported meshes
     textures/               # albedo, normal, roughness, metallic, ao, emissive
     materials/              # PBR material JSON (maps + scalars)
     shaders/                # assignable shader JSON (not a node graph yet)
     weather/                # UltraSky-style presets: clear, rain, snow, …
     audio/
   scripts/                  # .rhai (scripted track) or Rust (rust track)
   .bhippi/engine/engine-map.json   # AI digest, regenerated, never hand-authored
```

### Unreal analogue

| Unreal | Bhippi |
|---|---|
| Persistent Level / Game Mode | `assets/scenes/main.bscn.json` (`kind: "main"`) |
| UMG / HUD | `assets/scenes/hud.bscn.json` (`kind: "hud"`) + `UiDocument` on the player camera |
| Streaming / travel levels | `assets/scenes/level_XX.bscn.json` (`kind: "level"`) listed on Main and in the manifest |
| Content Browser | Engine Content Drawer (real files, not fake demo rows) |
| Details panel | Inspector (transform, mesh, material maps, shader) |
| Play In Editor | Toolbar Play. **Main Play** = Main + HUD + current/first level. **Level Play** = that level + HUD. |
| Replace a static mesh | Content Drawer → right-click mesh → **Replace Object** → pick a file from disk. Transform / materials / tags of the selected entity are preserved. |
| Material instance | `assets/materials/*.mat.json` with albedo/normal/roughness/metallic/ao/emissive |
| UltraSky / weather | `assets/weather/*.json` + scene `settings.weather`. Rain/snow/fog also spawn particles that hit meshes. |

### Play rules (do not break)

1. Double-click a **level** scene in the Content Drawer → that level opens in the viewport (edit mode).
2. Double-click **main** → Main opens. Play then runs the **full game** (Main + HUD overlay + first/selected level).
3. Double-click **hud** → HUD opens alone so the user can rearrange widgets without the 3D level in the way. Play on HUD previews the overlay only.
4. The user may change HUD, Main layout, and every level **manually**. The AI uses the same scene JSON and the same save path.
5. Switching project folder **must** drop the previous scene. If the new folder has no `Bhippi.game.toml`, the Engine chrome stays but the viewport is an **empty grid** — no demo subway, no fake assets, no auto-scaffold on save.

### Scene JSON (`bhippi-scene@1`)

- Every entity has a stable id (ULID in Rust scaffolds; `ent_*` is allowed in the live Three.js editor as long as it stays unique).
- Parent/child uses those ids, never names.
- Components the pipeline understands: `Transform`, `MeshRenderer`, `Light`, `Camera`, `RigidBody`, `Collider`, `CharacterController`, `UiDocument`, `ScriptRef`, `MaterialOverride`, `ShaderRef`, `WeatherVolume`.
- `settings.kind`: `main` | `level` | `hud` | `empty`.
- `settings.hud`: path to the HUD scene (Main only).
- `settings.levels`: ordered level paths (Main only).
- `settings.weather`: preset id (`clear`, `overcast`, `rain`, `snow`, `fog`, `storm`, `sunset`, `night`).

### AI generation flow (what chat-engine.md tells providers)

When the user says “make a game” / “add a level” / “change the HUD”:

1. Ensure `Bhippi.game.toml` exists (Engine New Game / `engine_create_game_manifest`). Do not scaffold just because Engine was opened.
2. Write **Main** first. Main references HUD + levels. Main owns the player spawn, the default camera, and the HUD attach (`UiDocument`).
3. Write **HUD** as its own scene. Widgets are entities tagged `hud`.
4. Write **each level** as its own `.bscn.json`. Never dump the whole game into one untitled scene.
5. Put meshes in `assets/models/`, textures in `assets/textures/` (one file per map), materials in `assets/materials/`. Reference them from `MeshRenderer.materials`.
6. After writes: scenes must parse, ids must exist, `default_scene` must point at Main, every `levels[]` path must exist.
7. Prefer one engine transaction / one logical file write per change so undo and the journal stay readable.

---

## 4. Session state — DONE vs REMAINING

Update these tables. Never delete a row; move it.

### Done in this pass (2026-08-30)

| ID | Area | What shipped |
|---|---|---|
| WB-01 | Workbench width | CSS `max-width: 900px` was the real ~50% cap. The pane can grow past 50%, then **hard-stops** at 68% of the split / chat floor 540px (owner screenshot 2026-08-30). Composer buttons stay on one row and must not overlap. Saved widths re-clamp on resize. |
| WB-02 | Effort slider | Cursor-like Effort popover: Faster↔Smarter, tick dots, white pill knob, particle fill, drag. Names stay Fast / Balanced / Quality / Ultra. |
| WB-03 | Browser | In-panel browser uses a **native Tauri webview** (not an iframe) so sites that send `X-Frame-Options` actually open. Iframe remains a fallback outside Tauri. Capabilities include `workbench-browser` + `browser-*` / `pip-*` windows. |
| WB-04 | Grok weekly | `probe_grok` now tries `grok -p /usage` (no model spend) then `grok dashboard`. Parses “weekly / left / used %”. Composer meter shows **% left** when a window exists; otherwise still **Not reported**. |
| WB-05 | App speed | Empty engine no longer builds a fake 18-entity demo; viewport RAF pauses when the pane is hidden / document hidden; splitter no longer fights a 900px CSS cap; workbench enter animation shortened. |
| ENG-A | Empty vs game | Non-game project: same Unreal chrome, **empty grid**, empty Content Drawer. Game project: **real** `assets/` listing. Switching projects clears the previous scene. Save no longer auto-scaffolds a game. |
| ENG-B | Unreal pipeline | New Game writes Main + HUD + level_01, weather presets, a lit PBR material, and `.bhippi/engine/engine-map.json`. Manifest gains `hud_scene` + `levels`. |
| ENG-C | Play / open | Double-click scene opens it. Play on Main composes Main+HUD+first level. Play on a level plays that level + HUD. HUD scene is independently editable. |
| ENG-D | Replace Object | Content Drawer context menu → Replace Object → OS file picker → copy into `assets/models` (or textures) → selected mesh keeps transform/scale/rotation/tags. |
| ENG-E | Materials / shaders | Inspector can assign albedo / normal / roughness / metallic / AO / emissive maps and a shader asset. Drag-drop of a `.mat.json` / `.shader.json` onto a mesh applies it. |
| ENG-F | Weather / lights | Create menu includes light types **and** UltraSky-style weather templates (clear, overcast, rain, snow, fog, storm, sunset, night). Weather changes sky, lights, and overlay particles. |
| ENG-G | AI map | `prompts/chat-engine.md` v2 describes Main/HUD/Levels. Engine status + mind-map digest expose the same shape. |
| ENG-023 | Viewport gizmos | Unreal-style RGB axis widget (top-right, click to snap camera). Selecting an actor shows a yellow box + TransformControls (W translate / E rotate / R scale / Q select). RMB look + WASD fly, MMB pan. Gizmo drag writes Transform. `chat-engine.md` v3. |
| ENG-023c | Snap / dup / undo | Grid snap 10 / 1 / 0.1 / Off; World/Local gizmo space (X); Ctrl+D duplicate; Delete; Ctrl+Z/Y undo stack in the Engine pane. |
| PROV-1 | Chat providers | Periodic detect no longer spawns CLI `--version`/`models` every 10s (that starved chat). Tick is local-server ports only; unchanged fingerprints skip the rebuild. |
| DOC-1 | This file | Living prompt created so the next agent does not rediscover the fake Content Drawer or the 900px cap. |

### Remaining (do these next; do not pretend they are done)

| ID | Priority | Work | Notes |
|---|---|---|---|
| ENG-100+ | **P0** | **Read `docs/13-ENGINE-AI-CONTROL-AND-UNREAL-UX-PLAN.md` first.** It audits the chat↔engine seam (findings F1–F9) and sequences the owner's goal — AI fully controls the engine · Unreal-grade editor · everything AI-generated is hand-editable · a real `bhippi-hud@1` HUD file · Play on Main actually runs the game — as phases `ENG-100…199`. | Phase 0 (one write path + journal + strip TS logic) is load-bearing; ENG-010 below is the ENG-168 decision in that plan. Tick boxes there against its Acceptance lines, never on looks. |
| ENG-010 | P1 | Bevy child-process viewport at 55 fps (ADR-0020 Option A, Windows `SetParent`) | Three.js in the webview is a stand-in (now has gizmos). Do not delete it until Bevy embeds. |
| ENG-023b | P1 | Unreal editor chrome: World Outliner folders, Details categories, Content Browser tiles, viewport toolbar (Perspective/Lit/Show) pixel-matched to UE5 | **mostly done 2026-09-01** — Outliner is a real tree (multi-select, drag-reparent, visibility/lock, filters incl. AI-made); Details is generated from the component registry with categories; viewport has Show flags + camera speed; command palette and Output Log added. Still open: Content Browser **tiles/thumbnails** and panel **docking**. See docs/13 Phase 4. |
| ENG-023c | P1 | Snap (grid 10 / 1 / 0.1), vertex snap, gizmo space world/local, duplicate (Alt-drag), delete | **done this slice** — grid snap 10/1/0.1/Off, World/Local (X), Ctrl+D duplicate, Delete, Ctrl+Z/Y undo in the Engine pane. Vertex snap + Alt-drag still remaining. |
| ENG-020 | P1 | Journaled `apply_transaction` into `engine_journal` (migration 0004) | **done 2026-09-01** — `bhippi-db::EngineRepo` + migration 0011 write every transaction (actor, label, ops, inverse, touched); `EngineSessions` puts human, UI and `<engine_action>` on one undo stack. See `docs/13-…` Phase 0. |
| ENG-021 | P1 | Parse `<engine_action>` in `chat.rs` like `<computer_action>` | **done, then upgraded 2026-09-01** — calls are now scanned out of the *live stream* (`<engine_action>` / `<engine_batch>` / `<engine_query>`), applied mid-turn as one transaction each, and fed back through a bounded read→act→verify loop; protocol text no longer appears in the answer. See `docs/13-…` Phase 1. |
| ENG-030 | P1 | Real GLB/GLTF GPU load + OBJ import with scale matching | Import now copies **and writes a `.meta.json` sidecar** (id + licence) — `docs/13-…` ENG-123. Still missing: the GPU loader (Phase 5) and OBJ/FBX→GLB conversion, which needs its own ADR for the `tobj`/`ufbx` dependency. |
| ENG-031 | P2 | Visual shader graph | ADR-0020 excluded this. File-based shaders ship (**`bhippi-shader@1` parsed and validated since 2026-09-01**); node graph still needs its own ADR. |
| ENG-032 | P2 | Weather affecting physics (wet friction, snow displacement) | Visual particles + lighting ship; material wetness is next. |
| ENG-033 | P2 | Multi-level travel at runtime (open door → load level_02, keep HUD) | Play currently composes Main+one level. |
| ENG-034 | P2 | Content Drawer thumbnails from real files | Names/types ship; no GPU thumbs yet. |
| WB-10 | P2 | Browser cookies / logins persist across app restarts | Native webview exists; profile partition not yet dedicated. |
| WB-11 | P2 | Grok weekly live-verified on a signed-in CLI | Parser + probe ship; run `cargo test -p bhippi-providers --test account_live -- --ignored` when the account is healthy. |
| PERF-1 | P2 | Drop Three.js from the main bundle when Engine has never been opened | Pane is lazy-mounted; the JS chunk still loads with the UI. |
| S1 | — | `chat_turns` persistence in `bhippi-db` | Unrelated but still the next ADE ticket. |

---

## 5. Files you will actually touch

| Concern | Path |
|---|---|
| Workbench split | `ui/src/App.tsx`, `ui/src/styles/workbench.css` |
| Browser | `ui/src/workbench/BrowserView.tsx`, `crates/bhippi-app/capabilities/main.json` |
| Effort slider | `ui/src/components/ComposerPopovers.tsx`, `ui/src/styles/chat.css` |
| Grok usage | `crates/bhippi-providers/src/account.rs`, `ui/src/components/ChatUsageMeter.tsx` |
| Engine UI | `ui/src/engine/*` |
| Engine domain | `crates/bhippi-engine/src/{scaffold,manifest,document,schema,mindmap}.rs` |
| Engine IPC | `crates/bhippi-app/src/engine.rs`, `crates/bhippi-app/src/files.rs` |
| AI doctrine | `prompts/chat-engine.md` |
| This tracker | `prompt.md`, `docs/PROGRESS.md` |

---

## 6. How to verify (honest)

```
[ ] Opening a non-game folder → Engine chrome, empty grid, empty Content Drawer
[ ] New Game → Main, HUD, level_01 on disk; Play on Main moves a player; HUD visible
[ ] Double-click level_01 → only that level in the viewport
[ ] Replace Object on a selected mesh → new file in assets/models, same transform
[ ] Drag a weather preset → sky/lights/particles change
[ ] Browser address `google.com` → page renders in the pane (Tauri desktop, not the iframe fallback)
[ ] Workbench drag past the old 900px / ~50% stop; chat still usable
[ ] Grok selected + signed in → meter shows weekly % left **or** Not reported (never a fake 0%)
[ ] cargo fmt, cargo clippy -D warnings, cargo test, tsc --noEmit
[ ] bindings regenerated if IPC changed
```

---

## 7. Definition of done for the *next* agent

Pick one Remaining row. Finish code + tests + this file + `docs/PROGRESS.md` session log. Do not start the Bevy viewport and the shader graph in the same change.

If you are continuing the owner’s original message: the items in **Done in this pass** are the ones that were requested for today. Remaining is the honest backlog, not a failure.
