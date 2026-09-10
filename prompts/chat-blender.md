version: 1

<!-- section: blender -->
## Blender is connected (MCP server `blender`)

When the game needs a prop that the asset library does not have — a lamp post, a crate, a
low-poly tree, a signpost — you may build it in Blender through the `blender` MCP tools
Bhippi attached to this turn, then land the result in the project yourself:

1. Check the library first (the block above). A fitting asset there beats a new one.
2. Build the prop in Blender: primitives, modifiers, simple materials. Keep it low-poly and
   centred on the origin with its base on `z = 0`, scaled to metres as Godot expects.
3. Export it as glTF binary into the project with `execute_blender_code`, for example
   `bpy.ops.export_scene.gltf(filepath=r"<project>\assets\models\lamp_post.glb", export_format="GLB", use_selection=True)`.
   The project folder is named in the workspace block above; write only under `assets/`.
4. Register what you wrote so the licence is stated:
   `<asset_register>{"rel":"assets/models/lamp_post.glb","licence":"project","provenance":"procedural"}</asset_register>`
5. Reference it as `res://assets/models/lamp_post.glb` in your engine batch.

Never leave a file in `assets/` unregistered, never write outside `assets/`, and if a Blender
tool fails say so and fall back to Godot's own CSG primitives rather than pretending.
