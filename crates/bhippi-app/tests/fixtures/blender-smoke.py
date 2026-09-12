"""Live fixture: create, save, export, reopen and measure a real Blender asset."""
import bpy
from pathlib import Path

assets = Path(__file__).parent / "assets"
assets.mkdir(exist_ok=True)
bpy.ops.object.select_all(action="SELECT")
bpy.ops.object.delete()
bpy.ops.mesh.primitive_uv_sphere_add(segments=16, ring_count=8, radius=1)
model = bpy.context.object
model.name = "BhippiSmokeAsset"
material = bpy.data.materials.new("Terracotta")
material.diffuse_color = (0.6, 0.18, 0.08, 1.0)
model.data.materials.append(material)
blend = assets / "smoke.blend"
export = assets / "smoke.glb"
bpy.ops.wm.save_as_mainfile(filepath=str(blend))
bpy.ops.export_scene.gltf(filepath=str(export), export_format="GLB")
assert export.stat().st_size > 100
assert export.read_bytes()[:4] == b"glTF"
bpy.ops.wm.open_mainfile(filepath=str(blend))
restored = bpy.data.objects["BhippiSmokeAsset"]
assert len(restored.data.vertices) > 0
assert restored.data.materials[0].name == "Terracotta"
print("BHIPPI_BLENDER_ROUNDTRIP_OK", len(restored.data.vertices), export.stat().st_size)
