extends CharacterBody2D

# Top-down player. The four movement actions are defined in project.godot; there is no
# gravity here on purpose — a top-down world has no down.

const SPEED := 220.0

# Looked up rather than named: `godot --check-only` does not register autoload singletons,
# so `BhippiProbe.set_var(...)` would fail the gate that proves this file compiles.
@onready var _probe: Node = get_node_or_null("/root/BhippiProbe")


func _physics_process(_delta: float) -> void:
	var input_dir := Input.get_vector("move_left", "move_right", "move_forward", "move_back")
	velocity = input_dir * SPEED
	move_and_slide()
	_publish()


func _publish() -> void:
	if _probe == null:
		return
	_probe.set_var("player_x", global_position.x)
	_probe.set_var("player_y", global_position.y)
