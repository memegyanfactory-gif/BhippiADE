extends CharacterBody3D

# Third-person player. Walks with move_left / move_right / move_forward / move_back and
# jumps with `jump`; all five actions are defined in project.godot.

const SPEED := 5.0
const JUMP_VELOCITY := 4.5

# Gravity comes from the project setting so changing it in the editor changes the game,
# rather than leaving a second number in here that quietly disagrees.
var _gravity: float = float(ProjectSettings.get_setting("physics/3d/default_gravity", 9.8))

# The probe is looked up by node path rather than named directly: `godot --check-only` does
# not register autoload singletons, so writing `BhippiProbe.set_var(...)` would make this
# file fail the very gate that is supposed to prove it compiles. Looking it up also means a
# build with the probe removed still runs.
@onready var _probe: Node = get_node_or_null("/root/BhippiProbe")


func _physics_process(delta: float) -> void:
	if not is_on_floor():
		velocity.y -= _gravity * delta
	elif Input.is_action_just_pressed("jump"):
		velocity.y = JUMP_VELOCITY

	var input_dir := Input.get_vector("move_left", "move_right", "move_forward", "move_back")
	var direction := Vector3(input_dir.x, 0.0, input_dir.y)
	if direction.length() > 0.0:
		direction = direction.normalized()
		velocity.x = direction.x * SPEED
		velocity.z = direction.z * SPEED
	else:
		velocity.x = move_toward(velocity.x, 0.0, SPEED)
		velocity.z = move_toward(velocity.z, 0.0, SPEED)

	move_and_slide()
	_publish()


func _publish() -> void:
	if _probe == null:
		return
	_probe.set_var("player_y", global_position.y)
	_probe.set_var("on_floor", is_on_floor())
