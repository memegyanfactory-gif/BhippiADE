version: 2

This workspace has no Godot project yet — there is no `project.godot` — so the engine
protocol is not live here. **You create the project.** Nobody presses a button for you.

```
<create_game>{"name": "3D Snake", "template": "empty_3d"}</create_game>
```

`template` is one of `empty_3d` (a lit 3D scene with a camera and a root script — the
default), `third_person_3d` (a walking, jumping character on a floor) or `top_down_2d` (a
four-direction 2D character). Pick the one nearest the game; you will change everything
anyway.

This runs the same typed scaffold the launcher uses: `project.godot`, `scenes/main.tscn`,
the probe autoload, the studio addon and the export presets, written into this workspace
by Rust. You do not write those files yourself — that write is refused (INV-088) — and you
do not ask the user to create the project, open a launcher, or press anything.

Emit the tag, say in one line what you are creating. The scaffold runs **in this turn** and
the engine vocabulary — `<engine_query>`, `<engine_batch>` — is live immediately. Query,
then emit the batches, until the game they asked for exists. Do not stop after the scaffold
and wait to be told "do it".

If the user's request already names the game, do not ask what to call it. If they said
"3D snake" (or any named genre), pick the obvious default (arcade wrap-around, `empty_3d`)
and build. Only ask with an `<ask_user>` card when you cannot pick and cannot build without
the answer — never for flavour you can decide.

Read `Plan/` before inventing a default. A `.docx` is a zip: extract `word/document.xml` with
Python `zipfile` (strip tags) rather than stopping because `tar`/`unzip` was denied. If a
`.md` sibling exists, read that.

After the scaffold, keep going until the game they asked for actually plays:

- `set_project_name` to the game's name (the scaffold keeps whatever `create_game` used).
- `set_main_scene` on the scene you built, so `Bhippi.game.toml` does not still point at
  `scenes/main.tscn`.
- Every Camera3D that should render has `current = true`. Godot 4 draws a grey void without
  it; a passing headless playtest does **not** prove Play shows anything.
- Do not leave the editor scene as empty Node3Ds. Either put MeshInstance3D primitives in
  the `.tscn` or mark the root script `@tool` and build a short preview in `_ready`.
- Query `gates` and clear `BHP-GD-415` / `BHP-GD-416` / `BHP-GD-417`.
- Playtest with the real actions, then tell the user to press **Play** in the studio
  viewport. **Preview** is the web export (`export/web`); it is empty until they export.
