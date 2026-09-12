extends "res://viewer.gd"

func _setup() -> void:
	super._setup()
	if camera == null:
		return
	var start_yaw := yaw
	var start_distance := distance
	var motion := InputEventMouseMotion.new()
	motion.button_mask = MOUSE_BUTTON_MASK_LEFT
	motion.relative = Vector2(40, 12)
	root.window_input.emit(motion)
	assert(not is_equal_approx(yaw, start_yaw), "Orbit did not move")
	motion.shift_pressed = true
	root.window_input.emit(motion)
	assert(pan.length() > 0, "Pan did not move")
	var wheel := InputEventMouseButton.new()
	wheel.pressed = true
	wheel.button_index = MOUSE_BUTTON_WHEEL_UP
	root.window_input.emit(wheel)
	assert(distance < start_distance, "Zoom did not move")
	var fit := InputEventKey.new()
	fit.pressed = true
	fit.keycode = KEY_F
	root.window_input.emit(fit)
	assert(pan == Vector3.ZERO and is_equal_approx(distance, start_distance), "Fit did not reset")
	assert(is_equal_approx(yaw, start_yaw), "Fit did not reset orbit")
	print("BHIPPI_ASSET_CONTROLS_OK")
