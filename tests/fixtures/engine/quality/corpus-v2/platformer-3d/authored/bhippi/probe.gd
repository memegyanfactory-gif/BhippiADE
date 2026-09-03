extends Node

# Bhippi playtest probe — registered as the BhippiProbe autoload.
#
# Two jobs, both driven entirely from the command line so a shipped game is unaffected:
#
#   --bhippi-inputs=<file>     replay scripted input, frame by frame
#   --bhippi-telemetry=<file>  append one JSON line per sample, then a final done line
#
# Without --bhippi-telemetry this node turns its own _process off in _ready and costs
# nothing. Nodes in the "bhippi_track" group are sampled; any script can publish state with
# BhippiProbe.set_var("health", 3) or mark a moment with BhippiProbe.emit_event("hit").
#
# Inputs file:
#   {"version": 1, "sample_every": 6,
#    "steps": [{"frame": 0, "action": "jump", "pressed": true},
#              {"frame": 10, "key": "KEY_W", "pressed": true}]}

const TRACK_GROUP := "bhippi_track"
const DEFAULT_SAMPLE_EVERY := 6
const INPUTS_ARG := "--bhippi-inputs="
const TELEMETRY_ARG := "--bhippi-telemetry="

var _frame: int = 0
var _sample_every: int = DEFAULT_SAMPLE_EVERY
var _telemetry_path: String = ""
var _steps: Array = []
var _next_step: int = 0
var _vars: Dictionary = {}
var _events: Array = []
var _file: FileAccess = null
var _closed: bool = false


func _ready() -> void:
	var args := OS.get_cmdline_user_args()
	_telemetry_path = _arg_value(args, TELEMETRY_ARG)
	if _telemetry_path.is_empty():
		set_process(false)
		return
	var inputs_path := _arg_value(args, INPUTS_ARG)
	if not inputs_path.is_empty():
		_load_inputs(inputs_path)
	_open_telemetry()


# Publish a value that every later telemetry line carries.
func set_var(key: String, value: Variant) -> void:
	_vars[key] = value


# Record a moment. Events are drained into the next telemetry line and then cleared.
func emit_event(event_name: String, data: Variant = null) -> void:
	_events.append({"frame": _frame, "name": event_name, "data": data})


func _arg_value(args: PackedStringArray, prefix: String) -> String:
	for arg in args:
		if arg.begins_with(prefix):
			return arg.substr(prefix.length())
	return ""


func _load_inputs(path: String) -> void:
	if not FileAccess.file_exists(path):
		push_warning("BhippiProbe: no inputs file at %s" % path)
		return
	var parsed: Variant = JSON.parse_string(FileAccess.get_file_as_string(path))
	if typeof(parsed) != TYPE_DICTIONARY:
		push_error("BhippiProbe: %s is not a JSON object" % path)
		return
	var document: Dictionary = parsed
	var steps: Variant = document.get("steps", [])
	if typeof(steps) == TYPE_ARRAY:
		_steps = steps
	if document.has("sample_every"):
		_sample_every = maxi(1, int(document["sample_every"]))


func _open_telemetry() -> void:
	# READ_WRITE keeps whatever is already in the file; WRITE_READ would truncate it.
	if FileAccess.file_exists(_telemetry_path):
		_file = FileAccess.open(_telemetry_path, FileAccess.READ_WRITE)
		if _file != null:
			_file.seek_end()
	else:
		_file = FileAccess.open(_telemetry_path, FileAccess.WRITE)
	if _file == null:
		push_error("BhippiProbe: could not open %s" % _telemetry_path)


func _process(_delta: float) -> void:
	_inject_for_frame()
	if _frame % _sample_every == 0:
		_write_line(_sample())
	_frame += 1


func _inject_for_frame() -> void:
	while _next_step < _steps.size():
		var raw: Variant = _steps[_next_step]
		if typeof(raw) != TYPE_DICTIONARY:
			_next_step += 1
			continue
		var step: Dictionary = raw
		if int(step.get("frame", 0)) > _frame:
			return
		_inject(step)
		_next_step += 1


func _inject(step: Dictionary) -> void:
	var pressed: bool = bool(step.get("pressed", true))
	if step.has("action"):
		var action := InputEventAction.new()
		action.action = StringName(str(step["action"]))
		action.pressed = pressed
		action.strength = 1.0 if pressed else 0.0
		Input.parse_input_event(action)
		return
	if step.has("key"):
		var key_name := str(step["key"]).trim_prefix("KEY_")
		var keycode := OS.find_keycode_from_string(key_name)
		if keycode == KEY_NONE:
			push_warning("BhippiProbe: unknown key %s" % key_name)
			return
		var event := InputEventKey.new()
		event.keycode = keycode
		event.physical_keycode = keycode
		event.pressed = pressed
		Input.parse_input_event(event)


func _sample() -> Dictionary:
	var tracked: Array = []
	for node in get_tree().get_nodes_in_group(TRACK_GROUP):
		tracked.append(_track(node))
	var scene_name := ""
	var current := get_tree().current_scene
	if current != null:
		scene_name = str(current.name)
	var line := {
		"frame": _frame,
		"time": Time.get_ticks_msec(),
		"fps": Engine.get_frames_per_second(),
		"scene": scene_name,
		"node_count": get_tree().get_node_count(),
		"tracked": tracked,
		"vars": _vars.duplicate(true),
		"events": _events.duplicate(true)
	}
	_events.clear()
	return line


func _track(node: Node) -> Dictionary:
	var entry := {"path": str(node.get_path())}
	var node3 := node as Node3D
	if node3 != null:
		entry["pos"] = [node3.global_position.x, node3.global_position.y, node3.global_position.z]
	var node2 := node as Node2D
	if node2 != null:
		entry["pos"] = [node2.global_position.x, node2.global_position.y]
	var body3 := node as CharacterBody3D
	if body3 != null:
		entry["vel"] = [body3.velocity.x, body3.velocity.y, body3.velocity.z]
	var body2 := node as CharacterBody2D
	if body2 != null:
		entry["vel"] = [body2.velocity.x, body2.velocity.y]
	return entry


func _write_line(payload: Dictionary) -> void:
	if _file == null:
		return
	_file.store_line(JSON.stringify(payload))
	_file.flush()


# The last line the reader looks for. Written once, whichever way the game ends.
func _finish() -> void:
	if _closed:
		return
	_closed = true
	_write_line({"done": true, "frames": _frame})
	if _file != null:
		_file.flush()
		_file = null


func _notification(what: int) -> void:
	if what == NOTIFICATION_WM_CLOSE_REQUEST:
		_finish()


func _exit_tree() -> void:
	_finish()
