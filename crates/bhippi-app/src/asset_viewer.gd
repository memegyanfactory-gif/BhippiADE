extends SceneTree
## Built-in read-only asset inspector. Godot owns loading, materials and rendering.

var camera: Camera3D
var orbit := Node3D.new()
var model: Node3D
var center := Vector3.ZERO
var radius := 1.0
var distance := 4.0
var yaw := 0.55
var pitch := -0.25
var pan := Vector3.ZERO
var screenshot_path := ""
var frames := 0

func _initialize() -> void:
	call_deferred("_setup")

func _setup() -> void:
	var args := OS.get_cmdline_user_args()
	if args.is_empty():
		_fail("No model was selected.")
		return
	root.msaa_3d = Viewport.MSAA_4X
	var document := GLTFDocument.new()
	var state := GLTFState.new()
	var error := document.append_from_file(args[0], state)
	if error != OK:
		_fail("The model could not be loaded. Check its mesh and texture files (error %s)." % error)
		return
	model = document.generate_scene(state) as Node3D
	if model == null:
		_fail("The file contains no 3D scene.")
		return
	var world := Node3D.new()
	root.add_child(world)
	world.add_child(model)
	var bounds := _bounds(model)
	if bounds.is_empty():
		_fail("The file contains no visible mesh geometry.")
		return
	var box: AABB = bounds[0]
	center = box.get_center()
	radius = maxf(box.size.length() * 0.5, 0.001)
	world.add_child(orbit)
	camera = Camera3D.new()
	orbit.add_child(camera)
	camera.fov = 40.0
	camera.near = maxf(radius * 0.001, 0.00001)
	camera.far = radius * 100.0
	camera.current = true
	var environment := WorldEnvironment.new()
	environment.environment = Environment.new()
	environment.environment.background_mode = Environment.BG_COLOR
	environment.environment.background_color = Color("202329")
	environment.environment.ambient_light_source = Environment.AMBIENT_SOURCE_COLOR
	environment.environment.ambient_light_color = Color("d7e3ff")
	environment.environment.ambient_light_energy = 0.35
	environment.environment.tonemap_mode = Environment.TONE_MAPPER_FILMIC
	world.add_child(environment)
	for lighting in [[Vector3(-45, -35, 0), 1.2, Color("fff0db")], [Vector3(-15, 135, 0), 0.45, Color("c1d9ff")], [Vector3(35, 45, 0), 0.5, Color.WHITE]]:
		var light := DirectionalLight3D.new()
		light.rotation_degrees = lighting[0]
		light.light_energy = lighting[1]
		light.light_color = lighting[2]
		world.add_child(light)
	_reset()
	root.size_changed.connect(_reset)
	root.window_input.connect(_input)
	var ui := CanvasLayer.new()
	root.add_child(ui)
	var reset := Button.new()
	reset.text = "Reset view · F"
	reset.position = Vector2(16, 16)
	reset.pressed.connect(_reset)
	ui.add_child(reset)
	if args.size() > 1:
		screenshot_path = args[1]
	print("BHIPPI_ASSET_READY")

func _bounds(node: Node) -> Array:
	var result: Array = []
	if node is MeshInstance3D and node.mesh != null:
		result.append(node.global_transform * node.get_aabb())
	for child in node.get_children():
		var found := _bounds(child)
		if not found.is_empty():
			if result.is_empty():
				result.append(found[0])
			else:
				result[0] = result[0].merge(found[0])
	return result

func _reset() -> void:
	if camera == null:
		return
	var aspect := maxf(float(root.size.x) / maxf(root.size.y, 1), 0.1)
	var half_fov := deg_to_rad(camera.fov * 0.5)
	var fit_angle := minf(half_fov, atan(tan(half_fov) * aspect))
	distance = radius / sin(fit_angle) * 1.15
	pan = Vector3.ZERO
	yaw = 0.55
	pitch = -0.25
	_update_camera()

func _update_camera() -> void:
	orbit.position = center + pan
	orbit.rotation = Vector3(pitch, yaw, 0)
	camera.position = Vector3(0, 0, distance)

func _input(event: InputEvent) -> void:
	if camera == null:
		return
	if event is InputEventMouseMotion:
		if event.button_mask & MOUSE_BUTTON_MASK_MIDDLE or (event.button_mask & MOUSE_BUTTON_MASK_LEFT and event.shift_pressed):
			pan += (-camera.global_basis.x * event.relative.x + camera.global_basis.y * event.relative.y) * distance * 0.0015
		elif event.button_mask & MOUSE_BUTTON_MASK_LEFT:
			yaw -= event.relative.x * 0.008
			pitch = clampf(pitch - event.relative.y * 0.008, -1.5, 1.5)
	if event is InputEventMouseButton and event.pressed:
		if event.button_index == MOUSE_BUTTON_WHEEL_UP:
			distance = maxf(distance * 0.88, radius * 0.1)
		elif event.button_index == MOUSE_BUTTON_WHEEL_DOWN:
			distance = minf(distance / 0.88, radius * 40.0)
	if event is InputEventKey and event.pressed and event.keycode == KEY_F:
		_reset()
	_update_camera()

func _process(_delta: float) -> bool:
	if not screenshot_path.is_empty():
		frames += 1
		if frames == 12:
			_capture.call_deferred()
	return false

func _capture() -> void:
	await RenderingServer.frame_post_draw
	var error := root.get_texture().get_image().save_png(screenshot_path)
	quit(error)

func _fail(message: String) -> void:
	print("BHIPPI_ASSET_ERROR: " + message)
	quit(1)
