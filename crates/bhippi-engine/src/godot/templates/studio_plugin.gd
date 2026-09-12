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
#    Bhippi rewrites `.bhippi/live/editor.json` whenever its attention moves; this plugin
#    polls it, and when the sequence number moves it follows. Two kinds of signal: an *edit*
#    means files changed, so it rescans, reloads the scene and selects the nodes that were
#    written; a *focus* means the agent is reading that scene or is about to change it, so it
#    opens the scene and disturbs nothing else. The focus half is what fills the long middle
#    of a turn, where the agent is deciding what to do and nothing has been written yet. See
#    `godot::live` in `bhippi-engine` for the writer and the file's shape (ADR-0050).
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
const SAVE_REQUEST_REL := ".bhippi/live/save_request"
const AUTO_SAVE_SECONDS := 1.5

# Godot restores its saved layout and scans the filesystem for a moment after a plugin loads.
# Opening a scene inside that window fights the restore, so the first-scene decision waits.
const FIRST_SCENE_DELAY := 1.5

# Every line this plugin prints starts with this, so Bhippi's Output pane can be read for
# "what did the editor do" without the engine's own chatter in the way.
const LOG_PREFIX := "[Bhippi Studio]"

# What preview mode hides, by the editor's own class names.
#
# These are not guesses. A throwaway plugin was run inside a real headless 4.7.1 editor and
# made it print its own control tree; every name here was read out of that dump. The first
# attempt at this matched button *text* — "Scene", "Project", "Output" — and hid nothing at
# all, because Godot 4 does not build those as buttons:
#
#   EditorTitleBar
#     MenuBar          menus=["Scene", "Project", "Debug", "Editor", "Help"]   <- one node
#     HBoxContainer    (the project-name label)
#     HBoxContainer 'EditorMainScreenButtons'   2D 3D Script Game "Asset Store"
#     EditorRunBar     (Godot's own play / pause / stop / movie-write)
#     HBoxContainer    (the renderer picker, "Forward+")
#   ...
#     EditorSceneTabs      (the scene tab strip)
#     EditorBottomPanel    (Output / Debugger / Audio / Animation / Shader Editor)
#
# A class name is also a far better key than a label: it does not move when the editor is
# translated, and it does not match a button somewhere else that happens to say "Audio".
const HIDE_CLASSES := ["EditorSceneTabs", "EditorBottomPanel"]

# The one child of the title bar that survives. Everything else there — the menus, the
# project label, Godot's own run bar, the renderer picker — goes, which also means a widget a
# future Godot adds to that bar is hidden by default. For preview mode that is the right
# default: the list of what to keep is short and deliberate, the list of what to hide is not.
const TITLE_BAR_KEEP := "EditorMainScreenButtons"

# Bhippi's other addon puts its search strip in the 3D editor's bottom container. It is ours,
# it has a stable name, and in preview mode it is one more row under the viewport.
const SKETCHFAB_NODE := "BhippiSketchfab"

# The menu titles, kept only so the scaffold's round-trip test can still say what this hides
# in the words a person would use. Nothing matches on them any more.
const MENU_NAMES := ["Scene", "Project", "Debug", "Editor", "Help"]

# Written when the toolbar's Play is pressed. Bhippi watches for it and runs the game in its
# own embedded surface; Godot's `play_main_scene()` would open a window Bhippi never launched
# and therefore cannot re-parent into the viewport, which is a game floating over the app.
const PLAY_REQUEST_REL := ".bhippi/live/play_request"

var _poll_timer: Timer = null
var _auto_save_timer: Timer = null
var _seen_seq: int = -1

# Preview mode (owner request): the viewport and the things that change what is in it, and
# nothing else. On by default — this editor is embedded inside Bhippi, where the chrome has
# never been the point.
var _preview_on := true
var _toolbar: HBoxContainer = null
var _preview_button: Button = null
# Remembered so the restore puts back exactly what was hidden, and touches nothing it did not
# hide. A control the walk never found stays absent from these and is never made visible.
var _hidden: Array[CanvasItem] = []


func _enter_tree() -> void:
	# Deferred by exactly one idle frame. Godot initialises plugins during its filesystem
	# scan, before it restores the saved editor layout ("Loading docks...", "Loading central
	# editor layout..."), so a value written inline here is set while the thing it controls is
	# still being rebuilt. One deferred call lands after that, and the mode is never re-asserted.
	_build_toolbar()
	_apply_preview_mode.call_deferred()

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

	_auto_save_timer = Timer.new()
	_auto_save_timer.name = "BhippiAutoSave"
	_auto_save_timer.wait_time = AUTO_SAVE_SECONDS
	_auto_save_timer.one_shot = false
	_auto_save_timer.autostart = true
	_auto_save_timer.timeout.connect(_auto_save)
	add_child(_auto_save_timer)

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
	_save_scenes()
	# Put the editor back the way it was found. A plugin that is switched off and leaves the
	# menus hidden has taken the project hostage.
	_preview_on = false
	_apply_preview_mode()
	if is_instance_valid(_toolbar):
		remove_control_from_container(CONTAINER_TOOLBAR, _toolbar)
		_toolbar.queue_free()
	_toolbar = null
	_preview_button = null
	if is_instance_valid(_poll_timer):
		_poll_timer.queue_free()
	_poll_timer = null
	if is_instance_valid(_auto_save_timer):
		_auto_save_timer.queue_free()
	_auto_save_timer = null


# ── the toolbar (owner request) ──────────────────────────────────────────────────────
#
# Two buttons, in the editor's own top bar rather than in a strip of Bhippi's above it: the
# viewport is a native child window and anything Bhippi draws over it is invisible, so the
# only place a control can sit *on* the preview is inside the editor itself.
func _build_toolbar() -> void:
	_toolbar = HBoxContainer.new()
	_toolbar.name = "BhippiToolbar"

	# An icon, not a word. Play is the one *action* in a row of view switches, and the editor
	# already owns the glyph for it — `MainPlay` is what Godot's own run bar uses, so it is the
	# right weight for this bar at every DPI and in every editor theme.
	var play := Button.new()
	play.tooltip_text = "Run the game in Bhippi's viewport"
	play.pressed.connect(_request_play)
	_style_like_main_screen(play)
	var play_icon := _editor_icon("MainPlay")
	if play_icon == null:
		play_icon = _editor_icon("Play")
	if play_icon != null:
		play.icon = play_icon
	else:
		# Never a nameless button: a theme without the glyph gets the word back.
		play.text = "Play"
	_toolbar.add_child(play)

	_preview_button = Button.new()
	_preview_button.toggle_mode = true
	_preview_button.button_pressed = _preview_on
	_preview_button.tooltip_text = "Hide everything except the viewport"
	_preview_button.toggled.connect(_on_preview_toggled)
	_style_like_main_screen(_preview_button)
	_toolbar.add_child(_preview_button)
	_sync_preview_button()

	add_control_to_container(CONTAINER_TOOLBAR, _toolbar)
	_seat_toolbar.call_deferred()


# Wear what 2D, 3D, Script, Game and Asset Store wear.
#
# `MainScreenButton` is the editor theme's own variation for exactly those buttons — read off
# them in a running editor, not guessed — so this is not an imitation of their look, it is
# their look, at every DPI and in every editor theme.
#
# Toggle state is left to the caller. The five switchers are toggles because one of them is
# always the current screen; Preview is a toggle for the same reason; Play is not, because it
# is an action and a button that stays pressed after it fires is a button that lies.
func _style_like_main_screen(button: Button) -> void:
	button.theme_type_variation = "MainScreenButton"
	button.focus_mode = Control.FOCUS_NONE


func _editor_icon(icon_name: String) -> Texture2D:
	var theme := EditorInterface.get_editor_theme()
	if theme == null or not theme.has_icon(icon_name, "EditorIcons"):
		return null
	return theme.get_icon(icon_name, "EditorIcons")


# Sit with the screen switcher rather than at the far end of the bar.
#
# `CONTAINER_TOOLBAR` appends to `EditorTitleBar`, which puts these buttons past the run bar
# and the renderer picker — the other side of the bar from the row they belong to. Moved to
# directly after the switcher, the two land inside the same centred cluster.
func _seat_toolbar() -> void:
	if not is_instance_valid(_toolbar):
		return
	var bar := _toolbar.get_parent()
	if bar == null:
		return
	var switcher := bar.find_child(TITLE_BAR_KEEP, false, false)
	if switcher == null:
		return
	bar.move_child(_toolbar, switcher.get_index() + 1)


# One word, like every other button on this row. Whether it is on is carried by the pressed
# state the `MainScreenButton` variation already draws — spelling it out in the label made
# this the only control up there wearing a sentence.
func _sync_preview_button() -> void:
	if not is_instance_valid(_preview_button):
		return
	_preview_button.text = "Preview"
	_preview_button.button_pressed = _preview_on
	_preview_button.tooltip_text = (
		"Showing the viewport only - click to bring the editor's panels back"
		if _preview_on
		else "Hide everything except the viewport"
	)


func _on_preview_toggled(pressed: bool) -> void:
	_preview_on = pressed
	_apply_preview_mode()


# Ask Bhippi to run the game. The scenes are flushed first, so what runs is what is on screen
# — the whole reason the editor saves before a run at all.
func _request_play() -> void:
	_save_scenes()
	var path := ProjectSettings.globalize_path("res://").path_join(PLAY_REQUEST_REL)
	var directory := path.get_base_dir()
	if not DirAccess.dir_exists_absolute(directory):
		DirAccess.make_dir_recursive_absolute(directory)
	var file := FileAccess.open(path, FileAccess.WRITE)
	if file == null:
		print("%s could not ask Bhippi to play (cannot write %s)" % [LOG_PREFIX, path])
		return
	file.store_string("play")
	file.close()
	print("%s asked Bhippi to run the game" % LOG_PREFIX)


# ── preview mode ─────────────────────────────────────────────────────────────────────
#
# What survives: the 3D viewport and its own tools, the main-screen switcher (2D, 3D, Script,
# Game, Asset Store) and this plugin's own two buttons. What goes: the Scene/Project/Debug/
# Editor/Help menus, the project label, Godot's own run bar, the renderer picker, the scene
# tab strip, every dock, the bottom panel, and Bhippi's own Sketchfab search strip.
#
# Everything is best-effort and reversible. There is no API for any of it, so the controls are
# found by walking the editor's own tree for their **class** — see `HIDE_CLASSES` for why a
# class and not a label, and for the tree this was read out of. A Godot that reorganises that
# tree loses the tidying and nothing else, which is the promise every other line in this
# plugin makes; each miss says so in the Output pane rather than failing silently.
func _apply_preview_mode() -> void:
	EditorInterface.set_distraction_free_mode(_preview_on)
	_apply_hidden_chrome()
	_sync_preview_button()
	print("%s preview mode %s" % [LOG_PREFIX, "on" if _preview_on else "off"])


func _apply_hidden_chrome() -> void:
	if _hidden.is_empty():
		_hidden = _collect_chrome()
	for control in _hidden:
		if is_instance_valid(control):
			control.visible = not _preview_on


# Everything preview mode hides, found once and remembered, so the restore puts back exactly
# what was taken and touches nothing else. A control the walk never found is never in this
# list and is therefore never made visible by us.
func _collect_chrome() -> Array[CanvasItem]:
	var found: Array[CanvasItem] = []
	var base := EditorInterface.get_base_control()
	if base == null:
		print("%s no base control; the editor keeps its chrome" % LOG_PREFIX)
		return found

	# The title bar, by subtraction: keep the main-screen switcher and this plugin's own
	# toolbar, hide its every other child — with one exception that is the whole reason the
	# row sits where it does.
	#
	# Two of those children expand (`SIZE_EXPAND`) and hold nothing but a label: they are the
	# springs either side of the switcher, and they are what centres it. Hiding them, which is
	# what the first version did, is why the buttons ended up adrift to one side. So an
	# expanding child is *kept* and its contents hidden instead — an empty spring still pushes.
	var title_bar := _find_by_class(base, "EditorTitleBar", 0)
	if title_bar != null:
		for child in title_bar.get_children():
			if child is not CanvasItem:
				continue
			if str(child.name) == TITLE_BAR_KEEP:
				continue
			# Never our own buttons, however the editor chose to wrap them.
			if is_instance_valid(_toolbar) and (child == _toolbar or child.is_ancestor_of(_toolbar)):
				continue
			var control := child as Control
			if control != null and (control.size_flags_horizontal & Control.SIZE_EXPAND) != 0:
				for inner in control.get_children():
					if inner is CanvasItem:
						found.append(inner)
				continue
			found.append(child)

	for class_name_wanted in HIDE_CLASSES:
		var node := _find_by_class(base, class_name_wanted, 0)
		if node != null:
			found.append(node)
		else:
			print("%s could not find %s; leaving it alone" % [LOG_PREFIX, class_name_wanted])

	var sketchfab := base.find_child(SKETCHFAB_NODE, true, false)
	if sketchfab is CanvasItem:
		found.append(sketchfab)

	print("%s preview mode hides %d controls" % [LOG_PREFIX, found.size()])
	return found


# The first descendant whose class is `wanted`. A class rather than a label: it survives the
# editor being translated, and it cannot collide with a button elsewhere that happens to
# carry the same word.
func _find_by_class(node: Node, wanted: String, depth: int) -> CanvasItem:
	if depth > 14:
		return null
	for child in node.get_children():
		if child.get_class() == wanted and child is CanvasItem:
			return child
		var found := _find_by_class(child, wanted, depth + 1)
		if found != null:
			return found
	return null


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
	_check_save_request()
	var data := _read_signal()
	if data.is_empty():
		return
	var seq := int(data.get("seq", -1))
	if seq <= _seen_seq:
		return
	_seen_seq = seq
	_show(data)


func _show(data: Dictionary) -> void:
	var scene := _scene_of(data)
	if scene == "" or not FileAccess.file_exists(scene):
		return

	# A signal says one of two things, and they call for different amounts of disturbance.
	#
	#   "focus" — the agent is reading this scene, or is about to change it. Nothing on disk
	#             has moved. Open it so the person can watch, and touch nothing else: no
	#             reload (which would discard an unsaved edit to answer a question nobody
	#             asked) and no change of selection.
	#   "edit"  — files changed underneath the editor. Rescan, reload, and select what moved.
	#
	# An older Bhippi wrote no kind at all and only ever wrote edits, so that is the default.
	var kind := str(data.get("kind", "edit"))
	var is_edit := kind != "focus"

	if is_edit:
		# Persist any in-editor manual changes so they are not lost on reload.
		_save_scenes()
		# New files — a script, a scene, an imported texture — do not exist for the editor
		# until it has scanned for them, and a child window may never get the focus that
		# triggers a scan of its own.
		var filesystem := EditorInterface.get_resource_filesystem()
		if filesystem != null and not filesystem.is_scanning():
			filesystem.scan()

	# One line per signal, and Bhippi collapses a repeated focus on the same scene before it
	# is ever written — so a turn that reads one scene forty times prints this once.
	print("%s %s %s — %s" % [
		LOG_PREFIX,
		"showing" if is_edit else "following",
		scene,
		str(data.get("label", "")),
	])

	var root := EditorInterface.get_edited_scene_root()
	var current := "" if root == null else root.scene_file_path
	if current == scene:
		if is_edit:
			# Already the scene on screen: the change is on disk, so take it from disk.
			EditorInterface.reload_scene_from_path(scene)
	else:
		var was_open := scene in EditorInterface.get_open_scenes()
		EditorInterface.open_scene_from_path(scene)
		if was_open and is_edit:
			# An open tab holds the copy loaded when it was opened, which is now stale.
			EditorInterface.reload_scene_from_path.call_deferred(scene)

	if is_edit:
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


func _auto_save() -> void:
	_save_scenes()


func _check_save_request() -> void:
	var req_path := ProjectSettings.globalize_path("res://").path_join(SAVE_REQUEST_REL)
	if FileAccess.file_exists(req_path):
		_save_scenes()
		DirAccess.remove_absolute(req_path)


func _save_scenes() -> void:
	if EditorInterface != null:
		var root := EditorInterface.get_edited_scene_root()
		if root != null:
			EditorInterface.save_all_scenes()
