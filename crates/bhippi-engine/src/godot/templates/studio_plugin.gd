@tool
# Bhippi Studio — the editor as Bhippi's viewport, following Bhippi's work.
#
# Why this exists: Bhippi's studio viewport *is* this Godot editor window, re-parented into
# Bhippi's own window (ADR-0045). That gives the plugin two jobs.
#
# 1. Get out of the way. Stock Godot fills that window with the Scene and Import docks on the
#    left, the Inspector and Node docks on the right, and FileSystem below them — so the hole
#    the studio reserved for the game shows mostly panels. This plugin turns on Godot's own
#    distraction-free mode once, when the editor loads it, which hides every dock and the
#    bottom panel and leaves the viewport, its toolbar, the menu bar and the scene tabs.
#
# 2. Show the work. The agent's typed actions land as file writes under a window that may
#    never gain focus, and Godot only rescans its filesystem on focus — so without this the
#    editor sat on whatever it happened to be showing while a level was built beside it.
#    Bhippi rewrites `.bhippi/live/editor.json` after every applied batch; this plugin polls
#    it, and when the sequence number moves it rescans the filesystem, opens or reloads the
#    scene the batch touched, and selects the nodes it wrote. See `godot::live` in
#    `bhippi-engine` for the writer and the file's shape (ADR-0050).
#
# What it never does: it does not write into the project, does not run the game, and does not
# re-assert the docks after you open them.
#
# How to get the docks back: press Ctrl+Shift+F12, or use the distraction-free toggle at the
# top-right of the main screen. Nothing here re-asserts the mode afterwards — no polling of
# that setting — so once you open the docks they stay open for the rest of the session.
#
# To stop all of this, untick "Bhippi Studio" in Project Settings → Plugins. The editor then
# behaves like stock Godot: docks visible, and no longer following Bhippi's changes.
#
# Note on unsaved editor edits: reloading a scene from disk discards unsaved changes to it.
# That is not a loss this plugin causes — Bhippi's typed actions read and rewrite the `.tscn`
# on disk, so an unsaved editor edit to the same scene was already going to be overwritten.
# Reloading makes that visible in the moment rather than at the next save.
#
# Written by Bhippi's scaffold (bhippi-engine::godot::scaffold), not by the agent: this is
# the same class of file as bhippi/probe.gd, so INV-088 does not apply to it.
extends EditorPlugin

# Must match `bhippi_engine::godot::live` — the scaffold's round-trip test pins them together.
const SIGNAL_REL := ".bhippi/live/editor.json"
const SIGNAL_VERSION := 1
const POLL_SECONDS := 0.25

# Godot restores its saved layout and scans the filesystem for a moment after a plugin loads.
# Opening a scene inside that window fights the restore, so the first-scene decision waits.
const FIRST_SCENE_DELAY := 1.5

# Every line this plugin prints starts with this, so Bhippi's Output pane can be read for
# "what did the editor do" without the engine's own chatter in the way.
const LOG_PREFIX := "[Bhippi Studio]"

var _poll_timer: Timer = null
var _seen_seq: int = -1


func _enter_tree() -> void:
	# Deferred by exactly one idle frame. Godot initialises plugins during its filesystem
	# scan, before it restores the saved editor layout ("Loading docks...", "Loading central
	# editor layout..."), so a value written inline here is set while the thing it controls is
	# still being rebuilt. One deferred call lands after that, and the mode is never re-asserted.
	_hide_the_docks.call_deferred()

	# Whatever is already on disk belongs to a previous session: it is read (so the first
	# scene decision below can use it) but never applied as if it had just happened.
	var existing := _read_signal()
	if not existing.is_empty():
		_seen_seq = int(existing.get("seq", -1))

	_poll_timer = Timer.new()
	_poll_timer.name = "BhippiLiveWatch"
	_poll_timer.wait_time = POLL_SECONDS
	_poll_timer.one_shot = false
	_poll_timer.autostart = true
	_poll_timer.timeout.connect(_poll)
	add_child(_poll_timer)

	# One line, once, into the Output pane Bhippi already streams. "Is the editor following?"
	# is otherwise unanswerable without opening the plugin list.
	print("%s watching %s from sequence %d" % [LOG_PREFIX, SIGNAL_REL, _seen_seq])

	var first := Timer.new()
	first.name = "BhippiFirstScene"
	first.wait_time = FIRST_SCENE_DELAY
	first.one_shot = true
	first.autostart = true
	first.timeout.connect(_open_first_scene.bind(first))
	add_child(first)


func _exit_tree() -> void:
	if is_instance_valid(_poll_timer):
		_poll_timer.queue_free()
	_poll_timer = null


func _hide_the_docks() -> void:
	EditorInterface.set_distraction_free_mode(true)


# ── the first scene ──────────────────────────────────────────────────────────────────

# A first-run editor opens with no scene at all, which is a grey viewport with a "no scene"
# hint in the middle of Bhippi's studio — the single most confusing thing a new project can
# show. So if nothing is open once the layout has settled, open the scene Bhippi last worked
# on, and failing that the project's own main scene. A session that restored its own tabs is
# left exactly as the user left it.
func _open_first_scene(timer: Timer) -> void:
	if is_instance_valid(timer):
		timer.queue_free()
	var root := EditorInterface.get_edited_scene_root()
	if root != null:
		print("%s first scene: %s was already open" % [LOG_PREFIX, root.scene_file_path])
		return
	var candidates: Array[String] = []
	var last_scene := _scene_of(_read_signal())
	if last_scene != "":
		candidates.append(last_scene)
	var main_scene := str(ProjectSettings.get_setting("application/run/main_scene", ""))
	if main_scene != "":
		candidates.append(main_scene)
	for path in candidates:
		if FileAccess.file_exists(path):
			print("%s first scene: opening %s (nothing was open)" % [LOG_PREFIX, path])
			EditorInterface.open_scene_from_path(path)
			return
	print("%s first scene: nothing to open (%d candidates)" % [LOG_PREFIX, candidates.size()])


# ── following Bhippi ─────────────────────────────────────────────────────────────────

func _poll() -> void:
	var data := _read_signal()
	if data.is_empty():
		return
	var seq := int(data.get("seq", -1))
	if seq <= _seen_seq:
		return
	_seen_seq = seq
	_show(data)


func _show(data: Dictionary) -> void:
	# New files — a script, a scene, an imported texture — do not exist for the editor until
	# it has scanned for them, and a child window may never get the focus that triggers one.
	var filesystem := EditorInterface.get_resource_filesystem()
	if filesystem != null and not filesystem.is_scanning():
		filesystem.scan()

	var scene := _scene_of(data)
	if scene == "" or not FileAccess.file_exists(scene):
		return

	# One line per applied batch — not per tick. Bhippi writes the signal once per journaled
	# transaction, so this is the same rate as the Versions list and belongs in the log beside it.
	print("%s showing %s — %s" % [LOG_PREFIX, scene, str(data.get("label", ""))])

	var root := EditorInterface.get_edited_scene_root()
	var current := "" if root == null else root.scene_file_path
	if current == scene:
		# Already the scene on screen: the change is on disk, so take it from disk.
		EditorInterface.reload_scene_from_path(scene)
	else:
		var was_open := scene in EditorInterface.get_open_scenes()
		EditorInterface.open_scene_from_path(scene)
		if was_open:
			# An open tab holds the copy loaded when it was opened, which is now stale.
			EditorInterface.reload_scene_from_path.call_deferred(scene)

	_select.call_deferred(data.get("focus_nodes", []))


# The nodes the batch wrote, selected so the Inspector shows what just changed and the
# viewport frames it. Paths are relative to the scene root, which is the shape
# `GodotAction::node_path` produces; "." is the root itself.
func _select(paths) -> void:
	if typeof(paths) != TYPE_ARRAY or paths.is_empty():
		return
	var root := EditorInterface.get_edited_scene_root()
	if root == null:
		return
	var selection := EditorInterface.get_selection()
	if selection == null:
		return
	var found: Array[Node] = []
	for entry in paths:
		if typeof(entry) != TYPE_STRING:
			continue
		var node := root.get_node_or_null(NodePath(entry))
		if node != null:
			found.append(node)
	if found.is_empty():
		# A removal names a node that is gone. Clearing here would leave the Inspector empty
		# for no reason, so the previous selection stands.
		return
	selection.clear()
	for node in found:
		selection.add_node(node)


# ── the file ─────────────────────────────────────────────────────────────────────────

# The signal, or an empty dictionary for "there is nothing to do". Every failure is that:
# the file is written whole by an atomic rename, so a read that fails is a file that is not
# there yet, and the next tick reads it. Nothing here reports an error into the editor's log,
# which would otherwise fill with one line every quarter second.
func _read_signal() -> Dictionary:
	var path := ProjectSettings.globalize_path("res://").path_join(SIGNAL_REL)
	if not FileAccess.file_exists(path):
		return {}
	var file := FileAccess.open(path, FileAccess.READ)
	if file == null:
		return {}
	var text := file.get_as_text()
	file.close()
	var parsed = JSON.parse_string(text)
	if typeof(parsed) != TYPE_DICTIONARY:
		return {}
	if int(parsed.get("version", 0)) != SIGNAL_VERSION:
		# A newer Bhippi writing a shape this addon does not know. Doing nothing is right:
		# the addon is replaced on the next workspace open, and a guess would be worse.
		return {}
	return parsed


# The signal's scene as a res:// path, or "" when it names none.
func _scene_of(data: Dictionary) -> String:
	if data.is_empty():
		return ""
	var scene = data.get("scene", null)
	if typeof(scene) != TYPE_STRING or scene == "":
		return ""
	if scene.begins_with("res://"):
		return scene
	return "res://" + scene
