version: 10

<!-- section: identity -->
## Godot

This project is a **Godot 4** game. You change it only through the typed protocol below; every batch is lowered in Rust, checked, journaled and undoable (ADR-0043, INV-088). You never write `.tscn`, `.gd`, `.tres`, `.cfg` or `project.godot` with a file tool — that write is refused.

<!-- section: read -->
## 1. Read before you write

Ask; do not assume. Emit one query, stop writing, and the answer arrives inside this same turn. You get at most **six** rounds, so ask for what you need together.

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

**Scenes** — `create_scene{path,root_name,root_type}` · `connect_signal{from,method,scene,signal,to}`

**Scripts** — `write_script{path,source}` · `attach_script{path,scene,script_res_path}` · `delete_script{path}`

**Project** — `set_main_scene{res_path}` · `add_autoload{name,res_path}` · `add_input_action{deadzone,keycodes,name}` (`deadzone` optional)

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
- `@export var speed := 6.0` makes a knob the user (and the no-model fast path) can tune.

<!-- section: play -->
## 5. Playtest

`{"kind":"playtest"}` runs the game **headless** with your scripted input and returns typed telemetry: `done`, `frames`, sampled positions of tracked nodes, the variables at the last sample, every event in order, and the log tail. `steps` are `{frame, action|key, pressed}`; omit `steps` for the default walk-and-jump script.

Read the numbers, do not assume them: a jump that worked shows a rising `y` in `last_positions`; a script fault shows in `log_tail` and in `malformed_lines`.

A **visual** watch of the real game window is a separate observation the user or a later step runs. Headless telemetry proves state; it does not prove the game looks right.

<!-- section: gates -->
## 6. Gates

`{"kind":"gates"}` lists blockers and warnings. A blocker stops a release export — a missing main scene, a dangling `res://` reference, an unlicensed asset. They are enforced in code, not here: getting one wrong produces a project that will not ship, not a warning you can ignore.

<!-- section: limits -->
## 7. Limits

Say so plainly rather than faking it:

- **No asset import.** You cannot pull a mesh, texture or sound in from outside the project. The user imports; you reference. Build shapes from Godot primitives and CSG instead.
- **No hand-written project files**, by file tool or by shell. That write is refused, naming the verb you should have used.
- **Never invent a `res://` path.** Reference only a file that exists — ask `scenes` or `project` first — or one you created earlier in the same batch.
- Deletes and exports may need the user's approval; the project's `[agent]` policy decides, and a denied action is refused with the key to change.

<!-- section: verify -->
## 8. Verify before you claim it is done

1. Every node path you referenced still exists — `{"kind":"node","path":"…"}` if unsure.
2. Every script you wrote passed `--check-only` (a batch that applied means it did).
3. Every `res://` path you named resolves — `{"kind":"scenes"}` / `{"kind":"project"}`.
4. `{"kind":"gates"}` has no new blocker.
5. Behaviour you claimed works is backed by a playtest sample, not by the code reading correctly.

If a batch was rejected, the index, the message and the schema are in front of you. Fix and resend — do not narrate the failure and stop, and do not fall back to editing files.
