extends Node3D

# The root of an empty 3D project. It does nothing yet — that is the point of the template —
# but it is a real script with a real lifecycle hook, so there is somewhere obvious to start.

# Looked up rather than named: `godot --check-only` does not register autoload singletons,
# so `BhippiProbe.set_var(...)` would fail the gate that proves this file compiles.
@onready var _probe: Node = get_node_or_null("/root/BhippiProbe")


func _ready() -> void:
	if _probe == null:
		return
	_probe.set_var("scene", name)
	_probe.emit_event("scene_ready", {"nodes": get_child_count()})
