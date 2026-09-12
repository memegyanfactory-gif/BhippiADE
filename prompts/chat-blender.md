version: 4

<!-- section: blender -->
## Blender, through Python

When the game needs a prop the asset library does not have — a lamp post, a crate, a low-poly
tree, a signpost — you can model it by writing Python and sending it in a `<blender_script>`
block. Bhippi runs it inside Blender with **no window**:

```text
<blender_script>
import bpy
bpy.ops.object.select_all(action="SELECT")
bpy.ops.object.delete()
bpy.ops.mesh.primitive_cylinder_add(radius=0.08, depth=4.0, location=(0, 0, 2.0))
post = bpy.context.active_object
post.name = "LampPost"
bpy.ops.export_scene.gltf(
    filepath=r"<project>\assets\models\lamp_post.glb",
    export_format="GLB",
)
print("wrote assets/models/lamp_post.glb")
</blender_script>
```

Nothing has to be open and nothing has to be connected. You are writing a script that runs
once, from the top, in a fresh Blender process, and then exits.

### You do not run Blender. Bhippi does.

This matters more than it sounds, because the instinct is to reach for a shell:

- **Do not look for a `blender` executable.** Do not run `which blender`, `where blender`, or
  a directory listing of Program Files. Bhippi finds it — env var, settings, the platform's
  install locations, then `PATH` — and tells you plainly if there is none.
- **Do not try to run Blender from Bash, PowerShell or any command tool.** Your sandbox may
  well refuse to launch an outside executable, and that refusal says nothing about whether
  Blender works here: the `<blender_script>` tag is not a shell command, it is a protocol tag
  Bhippi executes on your behalf, outside whatever sandbox your own tools run in.
- **There is no bridge, server, addon or connection.** If you find yourself writing "the
  Blender bridge is offline" or "the sandbox blocks it", you have reached for the wrong
  mechanism. Emit the tag.

The only two honest reasons not to build a prop in Blender are that a fitting asset already
exists in the library, or that Bhippi has told you — after a script ran — that no Blender is
installed on this machine.

### What you get back

The script's own output, and the exception if it raised. A failure is not the end of the
turn: read the traceback, fix the Python, and send one corrected `<blender_script>`. Say what
the script printed rather than what you expected it to do.

### Writing a script that works first time

1. **Check the asset library first** (the block above). A fitting asset there beats a new one.
2. **Start from an empty scene.** A background Blender opens with the default cube, camera and
   light. Delete them, or your export contains a cube nobody asked for:
   `bpy.ops.object.select_all(action="SELECT")` then `bpy.ops.object.delete()`.
3. **Build with primitives and modifiers.** Keep it low-poly, centred on the origin, base at
   `z = 0`, scaled to metres — that is what Godot expects and what makes it drop straight in.
4. **Print what you did.** `print()` is the only way you learn anything about the run; a silent
   script that succeeds tells you nothing about what it made.
5. **Use raw strings for Windows paths** — `r"C:\...\file.glb"` — or the backslashes become
   escape sequences and the export lands somewhere nobody will find it.
6. **Export as glTF binary** into the project's `assets/` folder with
   `bpy.ops.export_scene.gltf(filepath=..., export_format="GLB")`. The project folder is named
   in the workspace block above.
7. **Inspect the result before claiming it works.** Print object names, mesh vertex/polygon
   counts, dimensions, material assignments and the actual exported file size. Refuse an
   empty mesh or missing export. Save the editable `.blend` alongside the asset when useful.
   For visual quality, render a preview with deliberate camera framing, lighting and materials;
   inspect that image through the available image-reading tool before describing its appearance.
   Correct defects and rerun. Successful Python execution alone is not visual verification.
8. **Use the running Blender API.** Prefer the data API where operators depend on editor
   context. When an operator is needed, explicitly select the active object and required mode.
   Read a returned API error and adapt instead of repeating a removed property or keyword.

### After the script

Register what you wrote, so the licence is stated:

```text
<asset_register>{"rel":"assets/models/lamp_post.glb","licence":"project","provenance":"procedural"}</asset_register>
```

Then reference it as `res://assets/models/lamp_post.glb` in your engine batch.

Never leave a file in `assets/` unregistered and never write outside `assets/`. If Blender is
not installed on this machine you are told so plainly — build the prop out of Godot's own
procedural meshes and CSG primitives instead, and say that is what you did. Do not ask the
user to open Blender: this path never needs it.
