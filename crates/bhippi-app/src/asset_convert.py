"""Built-in preview conversion; never saves or changes the selected source."""
import json
from pathlib import Path
import bpy

settings = json.loads(Path(__file__).with_name("conversion.json").read_text(encoding="utf-8"))
source = Path(settings["source"])
destination = Path(settings["destination"])
kind = source.suffix.lower()
bpy.ops.wm.read_factory_settings(use_empty=True)
if kind == ".blend":
    bpy.ops.wm.open_mainfile(filepath=str(source), load_ui=False, use_scripts=False)
elif kind == ".obj":
    bpy.ops.wm.obj_import(filepath=str(source))
elif kind == ".fbx":
    bpy.ops.import_scene.fbx(filepath=str(source))
elif kind == ".stl":
    bpy.ops.wm.stl_import(filepath=str(source))
elif kind == ".ply":
    bpy.ops.wm.ply_import(filepath=str(source))
elif kind == ".dae" and hasattr(bpy.ops.wm, "collada_import"):
    bpy.ops.wm.collada_import(filepath=str(source))
else:
    raise RuntimeError(f"This Blender version cannot import {kind}. Export the model as GLB or glTF.")
if not any(obj.type == "MESH" for obj in bpy.context.scene.objects):
    raise RuntimeError("The model contains no mesh geometry to preview.")
bpy.ops.export_scene.gltf(filepath=str(destination), export_format="GLB", use_selection=False, export_cameras=False, export_lights=False)
if not destination.is_file() or destination.stat().st_size == 0:
    raise RuntimeError("The preview conversion did not produce a model.")
print("BHIPPI_ASSET_CONVERTED")
