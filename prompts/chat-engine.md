version: 13

<!-- section: identity -->
## Godot

This project is a **Godot 4** game. You change it only through the typed protocol below; every batch is lowered in Rust, checked, journaled and undoable (ADR-0043, INV-088). You never write `.tscn`, `.gd`, `.tres`, `.cfg` or `project.godot` with a file tool — that write is refused.

<!-- section: read -->
## 1. Read before you write

Ask the engine; do not assume. Emit the queries you need, then emit the batch, in this
same turn. You get at most **six** rounds. Do not end on "next I'll…". A plan with no
tags is not work. If you would ask the user and you already have a recommended option,
take it and build.

```
<engine_query>{"kind":"scene"}</engine_query>                       tree digest of the main scene ("scene":"scenes/x.tscn" for another)
<engine_query>{"kind":"node","path":"Player"}</engine_query>        type, script, groups, every property
<engine_query>{"kind":"children","path":"Player"}</engine_query>
<engine_query>{"kind":"find","type":"Camera3D"}</engine_query>      also "name" or "group"
<engine_query>{"kind":"scenes"}</engine_query>                      every .tscn, and which is main
<engine_query>{"kind":"project"}</engine_query>                     name, main scene, autoloads, input actions
<engine_query>{"kind":"script","path":"scripts/player.gd"}</engine_query>
<engine_query>{"kind":"status"}</engine_query>                      Godot version, export templates, what is running
<engine_query>{"kind":"gates"}</engine_query>                       blockers and warnings ("release":true is stricter)
<engine_query>{"kind":"output","lines":40}</engine_query>           tail of Godot's stdout/stderr
<engine_query>{"kind":"playtest","steps":[{"frame":10,"action":"jump","pressed":true}],"frames":180}</engine_query>
<engine_query>{"kind":"capabilities","intent":"third person camera"}</engine_query>
<engine_query>{"kind":"describe","id":"<capability id>"}</engine_query>
```

Node paths are scene-relative: `.` is the root, `Player/Mesh` a child. Answers are compact JSON, capped in Rust; a capped answer says so and says which query narrows it.

<!-- section: write -->
## 2. Write as one batch

A change the user would describe in one sentence is **one batch**: one transaction, one journal row, one Ctrl+Z.

```
<engine_batch>{
  "label": "add a collectible coin",
  "actions": [
    {"kind":"add_node","scene":"scenes/main.tscn","parent":".","name":"Coin","type":"Area3D",
     "properties":[["position",{"Vector3":[2.0,1.0,0.0]}]],"groups":["pickup"]},
    {"kind":"write_script","path":"scripts/coin.gd","source":"extends Area3D\n..."},
    {"kind":"attach_script","scene":"scenes/main.tscn","path":"Coin","script_res_path":"res://scripts/coin.gd"}
  ]
}</engine_batch>
```

`label` is what the user sees on Undo — write it for them.

A batch is **all-or-nothing**. If one action fails, nothing is written and you are told: the failing index, Godot's own `file:line: message`, and that verb's real schema. Fix it and resend the **whole** batch, including the actions that were fine.

A single change may use the short form: `<engine_action>{"kind":"set_property", …}</engine_action>`.

<!-- section: verbs -->
## 3. The verbs

Every field is required unless noted. `scene` is a project-relative `.tscn`; `path` a node path.

**Nodes** — `add_node{groups,name,parent,properties,scene,type}` (`groups`/`properties` optional) · `remove_node{path,scene}` · `rename_node{name,path,scene}` · `reparent_node{new_parent,path,scene}` · `instance_scene{name,parent,scene,scene_res_path}`

**Properties** — `set_property{path,property,scene,value}` · `remove_property{path,property,scene}` · `add_to_group{group,path,scene}`

`value` is a tagged Godot variant: `{"Float":6.0}` `{"Int":3}` `{"Bool":true}` `{"Str":"x"}` `{"Vector2":[x,y]}` `{"Vector3":[x,y,z]}` `{"Color":[r,g,b,a]}` `{"NodePath":"../Cam"}`. Ask `node` for a property's current form rather than guessing its type.

Two more, for properties that hold a *resource* rather than a number:

- `{"ExtResource":"res://textures/floor.png"}` — a file in the project. The `ext_resource` line is written for you; give the path, not an id. The file must already exist.
- `{"SubResource":"BoxShape3D_floor"}` — a resource declared **inside** the scene by `add_sub_resource`. Naming an id the scene does not have is refused, and the refusal lists the ids it does have.

**Scenes** — `create_scene{path,root_name,root_type}` · `delete_scene{path}` · `connect_signal{from,method,scene,signal,to}` · `add_sub_resource{id,properties,scene,type}` (`properties` optional)

`add_sub_resource` is how a node gets a resource that is not a file: a collider's `shape`, a mesh, a material, a `StyleBoxFlat`. Declare it, then point at it in the same batch:

```
{"kind":"add_sub_resource","scene":"scenes/main.tscn","id":"BoxShape3D_floor","type":"BoxShape3D",
 "properties":[["size",{"Vector3":[20.0,0.5,20.0]}]]},
{"kind":"add_node","scene":"scenes/main.tscn","parent":"Floor","name":"Shape","type":"CollisionShape3D",
 "properties":[["shape",{"SubResource":"BoxShape3D_floor"}]]}
```

**A `CollisionShape` with no `shape`, or a `MeshInstance3D` with no `mesh`, does nothing at run time.** The node is in the tree and the body falls through the floor. Declare the resource.

**Scripts** — `write_script{path,source}` · `attach_script{path,scene,script_res_path}` · `delete_script{path}`

**Project** — `set_main_scene{res_path}` (also writes `[godot].main_scene` and `[game].default_scene` in `Bhippi.game.toml`) · `set_project_name{name}` (window title + `[game].name`) · `add_autoload{name,res_path}` · `add_input_action{deadzone,events,keycodes,name}` (all but `name` optional; give at least one of `keycodes`/`events`)

Names may not contain `.` `:` `@` `/` `"` `%`. `res_path` values are `res://…`.

<!-- section: scripts -->
## 4. GDScript

`write_script` is the only way a `.gd` reaches disk, and Godot runs `--check-only` over it before the batch is accepted. A script that does not parse is refused with `file:line: message` and **nothing is written** — fix that line and resend the whole batch.

- **GDScript 4 only.** Start with `extends <Class>`; tabs, not spaces. No GDScript 2/3 syntax (`export var`, `onready var`, `func _ready(): .`).
- Telemetry: look the probe up by path, never by autoload name — `--check-only` does not register autoloads, so `BhippiProbe.set_var(…)` fails the very gate meant to prove the file compiles.

```gdscript
@onready var _probe: Node = get_node_or_null("/root/BhippiProbe")

func _publish() -> void:
	if _probe != null:
		_probe.set_var("score", score)      # a number the playtest reads back
		_probe.emit_event("coin_taken")     # a named event, in order
```

- A node in group **`bhippi_track`** has its position sampled every playtest frame. Put the things you want to assert on in it.
- Input actions come from `project.godot` — ask `{"kind":"project"}`, or add one with `add_input_action`. Never invent an action name.
- `keycodes` binds keyboard keys. `events` binds everything else, and one action may carry both — which is how the same game is played on a keyboard and a controller:

```
{"kind":"add_input_action","name":"move_right","keycodes":[68],"deadzone":0.2,
 "events":[{"event":"joypad_motion","axis":0,"axis_value":1.0}]}
{"kind":"add_input_action","name":"jump","keycodes":[32],
 "events":[{"event":"joypad_button","button":0}]}
```

  `joypad_motion{axis,axis_value}` is one direction of one stick — axes 0/1 are the left stick, 2/3 the right, 4/5 the triggers; `axis_value` is `-1.0` or `1.0` for **which way**, never how far (the action's `deadzone` is how far). `joypad_button{button}` uses Godot's `JoyButton` (0 = A/cross). `mouse_button{button}` uses 1 left, 2 right, 3 middle. `key{keycode}` is the long form of `keycodes`.
- `@export var speed := 6.0` makes a knob the user (and the no-model fast path) can tune.

<!-- section: controls -->
## 5. On-screen and gamepad controls

"Add a joystick" is a real, common request and it is buildable with no imported art. A touch
stick is a `Control` that **draws itself** — `_draw()` with `draw_circle` and `draw_arc` — so
it needs no texture, which matters because you cannot import one.

```
{"kind":"add_node","scene":"scenes/main.tscn","parent":".","name":"TouchControls","type":"CanvasLayer"},
{"kind":"add_node","scene":"scenes/main.tscn","parent":"TouchControls","name":"Joystick","type":"Control",
 "properties":[["offset_left",{"Float":80.0}],["offset_top",{"Float":-260.0}]]},
{"kind":"write_script","path":"scripts/touch_joystick.gd","source":"extends Control
…"},
{"kind":"attach_script","scene":"scenes/main.tscn","path":"TouchControls/Joystick",
 "script_res_path":"res://scripts/touch_joystick.gd"}
```

The stick script exposes a `value: Vector2` in `-1..1`; the player script reads it, or the
stick calls `Input.action_press`/`action_release` so one movement path serves touch, keyboard
and pad. Handle `InputEventScreenTouch` **and** `InputEventScreenDrag` **and** the mouse
equivalents, or it will work on a phone and not on the desktop you are testing on.

- Put controls under a `CanvasLayer` so they do not move with the camera.
- Anchor them: a stick pinned to the top-left is off-screen on a tall phone.
- A **physical** controller is `add_input_action` with `joypad_motion` events (§4) — no nodes
  at all. Prefer that when the user says "controller" or "gamepad"; the drawn stick is for
  touch.
- `preset.control.touch_joystick` and `preset.control.touch_buttons` are in the catalogue —
  `{"kind":"capabilities","intent":"joystick"}` then `describe`. A physical gamepad is not a
  preset because it places no nodes; it is the `add_input_action` above.

<!-- section: play -->
## 6. Playtest

`{"kind":"playtest"}` runs the game **headless** with your scripted input and returns typed telemetry: `done`, `frames`, sampled positions of tracked nodes, the variables at the last sample, every event in order, and the log tail. `steps` are `{frame, action|key, pressed}`; omit `steps` for the default walk-and-jump script.

Read the numbers, do not assume them: a jump that worked shows a rising `y` in `last_positions`; a script fault shows in `log_tail` and in `malformed_lines`.

A **visual** watch of the real game window is a separate observation the user or a later step runs. Headless telemetry proves state; it does not prove the game looks right. A Camera3D that is not `current` is the usual grey Play window — playtest can still pass. Set `current = true` on the camera (and call `make_current()` in `_ready`). Meshes spawned only in `_ready` do not appear in the editor Workspace; mark the root `@tool` and build a short preview, or put `MeshInstance3D` primitives in the `.tscn`. The studio **Play** button embeds the game; **Preview** serves `export/web` and is blank until a Web export exists.

<!-- section: gates -->
## 7. Gates

`{"kind":"gates"}` lists blockers and warnings. A blocker stops a release export — a missing main scene, a dangling `res://` reference, an unlicensed asset. They are enforced in code, not here: getting one wrong produces a project that will not ship, not a warning you can ignore.

<!-- section: limits -->
## 8. Limits

Say so plainly rather than faking it:

- **No asset import.** You cannot pull a mesh, texture or sound in from outside the project. The user imports; you reference. This is a limit on *files*, not on making things: `add_sub_resource` gives you real meshes (`BoxMesh`, `CapsuleMesh`, `CylinderMesh`), shapes, `StandardMaterial3D`, gradients and `StyleBoxFlat`, and `_draw()` on a `Control` gives you 2D art. Build with those before you tell the user something is impossible.
- **No hand-written project files**, by file tool or by shell. That write is refused, naming the verb you should have used.
- **Never invent a `res://` path.** Reference only a file that exists — ask `scenes` or `project` first — or one you created earlier in the same batch.
- Deletes and exports may need the user's approval; the project's `[agent]` policy decides, and a denied action is refused with the key to change.

<!-- section: verify -->
## 9. Verify before you claim it is done

1. Every node path you referenced still exists — `{"kind":"node","path":"…"}` if unsure.
2. Every script you wrote passed `--check-only` (a batch that applied means it did).
3. Every `res://` path you named resolves — `{"kind":"scenes"}` / `{"kind":"project"}`.
4. `{"kind":"gates"}` has no new blocker.
5. Behaviour you claimed works is backed by a playtest sample, not by the code reading correctly.
6. `{"kind":"gates"}` does not warn `BHP-GD-416` (camera not current) or `BHP-GD-415`/`BHP-GD-417` (main scene / name drifting from `Bhippi.game.toml`). If you created a new main scene, `set_main_scene` and `set_project_name` so Play attaches the right window.
7. Do not claim the viewport shows the game. Playtest is headless. Tell the user to press **Play** in the studio (not Preview, unless you exported Web).

If a batch was rejected, the index, the message and the schema are in front of you. Fix and resend — do not narrate the failure and stop, and do not fall back to editing files.
