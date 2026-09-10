@tool
# Bhippi Sketchfab — the asset library, inside the editor.
#
# What this is: a translucent strip along the bottom of the 3D (or 2D) viewport holding a
# scrollable row of Sketchfab models — thumbnail, name, author, licence chip — with a search
# box and one button per card. It follows the editor: when you switch between the 3D and 2D
# main screens the strip moves with you, and it collapses to a single header bar when you
# want the viewport back.
#
# What it is *not*: it is not a Sketchfab client. This file makes no HTTP request, holds no
# token, and decides nothing about licences. It is a **view over a file**, exactly like
# `bhippi_studio/plugin.gd` is a view over `.bhippi/live/editor.json`:
#
#   Bhippi writes  →  .bhippi/live/sketchfab.json          →  this panel draws it
#   this panel writes → .bhippi/live/sketchfab_request.json →  Bhippi acts on it
#
# Every search, every download, every licence ruling and the credential itself live in Rust,
# on the other side of that file (`bhippi-engine::godot::sketchfab`, `bhippi-app::sketchfab`).
# There are three reasons it is built this way and none of them is taste:
#
#   1. A token in a GDScript file is a token in the user's project folder, in their git
#      history, and in any export. Bhippi keeps it in the OS keychain (INV-037).
#   2. `project.godot` lists this addon, so a project can be shared. An addon that could
#      search and download would be an addon that could do so for whoever opened the project.
#   3. A licence rule that decides whether a model may ship belongs somewhere with tests and
#      a release gate behind it, not in a file the editor hot-reloads.
#
# So the panel is deliberately dumb: it renders what it is given and it posts what was
# clicked. If Bhippi is not running, nothing happens when you click — the request file sits
# there until it is, which is the honest behaviour for a view with no back end.
#
# Written by Bhippi's scaffold (bhippi-engine::godot::scaffold), not by the agent: the same
# class of file as bhippi/probe.gd, so INV-088 does not apply to it.
#
# To switch it off: untick "Bhippi Sketchfab" in Project Settings → Plugins. The strip goes
# and the editor is stock again.
extends EditorPlugin

# Must match `bhippi_engine::godot::sketchfab` — `the_addon_and_the_channel_agree` pins them.
const STATE_REL := ".bhippi/live/sketchfab.json"
const REQUEST_REL := ".bhippi/live/sketchfab_request.json"
const CHANNEL_VERSION := 1
const POLL_SECONDS := 0.4

# Every line this addon prints starts with this, so Bhippi's Output pane can be read for
# "what did the library do" without the editor's own chatter in the way.
const LOG_PREFIX := "[Bhippi Sketchfab]"

const CARD_WIDTH := 132
const CARD_HEIGHT := 158
const THUMB_HEIGHT := 84
const STRIP_HEIGHT := 196
const COLLAPSED_HEIGHT := 30

var _root: PanelContainer = null
var _search_box: LineEdit = null
var _shippable_check: CheckBox = null
var _animated_check: CheckBox = null
var _status_label: Label = null
var _account_label: Label = null
var _connect_button: Button = null
var _collapse_button: Button = null
var _strip: HBoxContainer = null
var _scroll: ScrollContainer = null
var _timer: Timer = null

var _seen_seq: int = -1
var _container: int = -1
var _collapsed: bool = false
# Thumbnails are re-read from disk only when the file changes underneath us. Without this
# the panel would decode every JPEG on the strip four times a second.
var _texture_cache: Dictionary = {}


func _enter_tree() -> void:
	_root = _build_panel()
	_move_to_container(EditorPlugin.CONTAINER_SPATIAL_EDITOR_BOTTOM)

	_timer = Timer.new()
	_timer.name = "BhippiSketchfabWatch"
	_timer.wait_time = POLL_SECONDS
	_timer.one_shot = false
	_timer.autostart = true
	_timer.timeout.connect(_poll)
	add_child(_timer)

	print("%s watching %s" % [LOG_PREFIX, STATE_REL])
	# Draw whatever is already on disk rather than showing an empty strip for the first
	# 400 ms of every editor session.
	var existing := _read_state()
	if not existing.is_empty():
		_seen_seq = int(existing.get("seq", -1))
		_render(existing)


func _exit_tree() -> void:
	if is_instance_valid(_timer):
		_timer.queue_free()
	_timer = null
	if _container != -1 and is_instance_valid(_root):
		remove_control_from_container(_container, _root)
	if is_instance_valid(_root):
		_root.queue_free()
	_root = null
	_texture_cache.clear()


# The strip follows whichever main screen you are on, so a 2D project sees it too. A Control
# has one parent, so this is a move rather than a second panel.
func _main_screen_changed(screen: String) -> void:
	if screen == "3D":
		_move_to_container(EditorPlugin.CONTAINER_SPATIAL_EDITOR_BOTTOM)
	elif screen == "2D":
		_move_to_container(EditorPlugin.CONTAINER_CANVAS_EDITOR_BOTTOM)


func _move_to_container(container: int) -> void:
	if container == _container or not is_instance_valid(_root):
		return
	if _container != -1:
		remove_control_from_container(_container, _root)
	_container = container
	add_control_to_container(_container, _root)


# ── the panel ────────────────────────────────────────────────────────────────────────

func _build_panel() -> PanelContainer:
	var panel := PanelContainer.new()
	panel.name = "BhippiSketchfab"
	panel.custom_minimum_size = Vector2(0, STRIP_HEIGHT)
	panel.mouse_filter = Control.MOUSE_FILTER_STOP

	# Translucent, so the viewport reads through it and the strip feels like part of the
	# scene rather than another dock bolted underneath.
	var style := StyleBoxFlat.new()
	style.bg_color = Color(0.06, 0.07, 0.09, 0.72)
	style.border_color = Color(1.0, 1.0, 1.0, 0.08)
	style.set_border_width_all(1)
	style.set_corner_radius_all(6)
	style.set_content_margin_all(6)
	panel.add_theme_stylebox_override("panel", style)

	var column := VBoxContainer.new()
	column.add_theme_constant_override("separation", 4)
	panel.add_child(column)

	column.add_child(_build_header())

	_scroll = ScrollContainer.new()
	_scroll.horizontal_scroll_mode = ScrollContainer.SCROLL_MODE_AUTO
	_scroll.vertical_scroll_mode = ScrollContainer.SCROLL_MODE_DISABLED
	_scroll.size_flags_vertical = Control.SIZE_EXPAND_FILL
	column.add_child(_scroll)

	_strip = HBoxContainer.new()
	_strip.add_theme_constant_override("separation", 6)
	_scroll.add_child(_strip)

	return panel


func _build_header() -> HBoxContainer:
	var header := HBoxContainer.new()
	header.add_theme_constant_override("separation", 6)

	_collapse_button = Button.new()
	_collapse_button.text = "▾"
	_collapse_button.tooltip_text = "Collapse the Sketchfab strip"
	_collapse_button.flat = true
	_collapse_button.pressed.connect(_toggle_collapsed)
	header.add_child(_collapse_button)

	var title := Label.new()
	title.text = "Sketchfab"
	header.add_child(title)

	_account_label = Label.new()
	_account_label.modulate = Color(1, 1, 1, 0.6)
	header.add_child(_account_label)

	_search_box = LineEdit.new()
	_search_box.placeholder_text = "Search models — try \"low poly knight\""
	_search_box.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_search_box.custom_minimum_size = Vector2(160, 0)
	_search_box.text_submitted.connect(_on_search_submitted)
	header.add_child(_search_box)

	var search_button := Button.new()
	search_button.text = "Search"
	search_button.pressed.connect(func() -> void: _on_search_submitted(_search_box.text))
	header.add_child(search_button)

	_shippable_check = CheckBox.new()
	_shippable_check.text = "Shippable only"
	_shippable_check.button_pressed = true
	_shippable_check.tooltip_text = "Only licences that can survive a Release export (CC0, CC-BY, CC-BY-SA, CC-BY-ND)."
	header.add_child(_shippable_check)

	_animated_check = CheckBox.new()
	_animated_check.text = "Animated"
	_animated_check.tooltip_text = "Only models that carry animation."
	header.add_child(_animated_check)

	_connect_button = Button.new()
	_connect_button.text = "Connect"
	_connect_button.pressed.connect(_on_connect_pressed)
	header.add_child(_connect_button)

	_status_label = Label.new()
	_status_label.modulate = Color(1, 1, 1, 0.7)
	_status_label.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_status_label.clip_text = true
	header.add_child(_status_label)

	return header


func _toggle_collapsed() -> void:
	_collapsed = not _collapsed
	_scroll.visible = not _collapsed
	_collapse_button.text = "▸" if _collapsed else "▾"
	_root.custom_minimum_size = Vector2(
		0, COLLAPSED_HEIGHT if _collapsed else STRIP_HEIGHT
	)


# ── drawing what Bhippi wrote ────────────────────────────────────────────────────────

func _poll() -> void:
	var state := _read_state()
	if state.is_empty():
		return
	var seq := int(state.get("seq", -1))
	if seq <= _seen_seq:
		return
	_seen_seq = seq
	_render(state)


func _render(state: Dictionary) -> void:
	var connection := str(state.get("connection", "signed_out"))
	var account := str(state.get("account", ""))
	var busy := bool(state.get("busy", false))
	var error := str(state.get("error", ""))
	var status := str(state.get("status", ""))

	_account_label.text = account if connection == "connected" else ""
	_connect_button.text = "Sign out" if connection == "connected" else "Connect"
	_connect_button.disabled = connection == "connecting"

	# One line, and the error wins: a strip that shows results while quietly having failed
	# to fetch the newest ones is a screen that lies.
	if error != "":
		_status_label.text = error
		_status_label.modulate = Color(1.0, 0.55, 0.45)
	else:
		_status_label.text = status if busy or status != "" else ""
		_status_label.modulate = Color(1, 1, 1, 0.7)

	var query := str(state.get("query", ""))
	if query != "" and not _search_box.has_focus():
		_search_box.text = query

	for child in _strip.get_children():
		child.queue_free()

	var results = state.get("results", [])
	if typeof(results) != TYPE_ARRAY or results.is_empty():
		var empty := Label.new()
		if connection != "connected":
			empty.text = "Connect to Sketchfab to browse models."
		elif busy:
			empty.text = "Searching…"
		elif query == "":
			empty.text = "Search for a model, or ask Bhippi in chat."
		else:
			empty.text = "Nothing matched \"%s\"." % query
		empty.modulate = Color(1, 1, 1, 0.55)
		_strip.add_child(empty)
		return

	for entry in results:
		if typeof(entry) == TYPE_DICTIONARY:
			_strip.add_child(_build_card(entry))


func _build_card(entry: Dictionary) -> Control:
	var uid := str(entry.get("uid", ""))
	var usage := str(entry.get("usage", "unknown"))
	var imported = entry.get("imported_rel", null)
	var in_project: bool = typeof(imported) == TYPE_STRING and imported != ""

	var card := PanelContainer.new()
	card.custom_minimum_size = Vector2(CARD_WIDTH, CARD_HEIGHT)
	card.tooltip_text = "%s\n%s" % [str(entry.get("name", "")), str(entry.get("note", ""))]

	var style := StyleBoxFlat.new()
	style.bg_color = Color(1, 1, 1, 0.04)
	style.set_corner_radius_all(4)
	style.set_content_margin_all(4)
	card.add_theme_stylebox_override("panel", style)

	var column := VBoxContainer.new()
	column.add_theme_constant_override("separation", 2)
	card.add_child(column)

	var thumb := TextureRect.new()
	thumb.custom_minimum_size = Vector2(0, THUMB_HEIGHT)
	thumb.expand_mode = TextureRect.EXPAND_IGNORE_SIZE
	thumb.stretch_mode = TextureRect.STRETCH_KEEP_ASPECT_COVERED
	thumb.texture = _thumbnail(str(entry.get("thumbnail_path", "")))
	column.add_child(thumb)

	var name_label := Label.new()
	name_label.text = str(entry.get("name", "Untitled"))
	name_label.clip_text = true
	name_label.custom_minimum_size = Vector2(CARD_WIDTH - 8, 0)
	column.add_child(name_label)

	var meta := Label.new()
	var faces := int(entry.get("face_count", 0))
	var animated := bool(entry.get("is_animated", false))
	meta.text = "%s · %s tris%s" % [
		str(entry.get("licence_label", "unknown")),
		_thousands(faces),
		" · anim" if animated else "",
	]
	meta.clip_text = true
	# The licence is the one thing on this card that decides whether the game can ship, so
	# it is coloured rather than left as one more grey line: green ships, amber imports but
	# blocks a Release, red cannot be imported at all.
	if usage == "allowed":
		meta.modulate = Color(0.55, 0.85, 0.6)
	elif usage == "refused":
		meta.modulate = Color(1.0, 0.5, 0.45)
	else:
		meta.modulate = Color(1.0, 0.78, 0.4)
	column.add_child(meta)

	var buttons := HBoxContainer.new()
	buttons.add_theme_constant_override("separation", 2)
	column.add_child(buttons)

	var add := Button.new()
	add.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	if in_project:
		add.text = "In project"
		add.disabled = true
		add.tooltip_text = "res://%s" % str(imported)
	elif usage == "refused":
		add.text = "Blocked"
		add.disabled = true
		add.tooltip_text = str(entry.get("note", ""))
	else:
		add.text = "Add"
		add.pressed.connect(func() -> void: _request({"action": "import", "uid": uid}))
	buttons.add_child(add)

	var open := Button.new()
	open.text = "↗"
	open.tooltip_text = "Open on Sketchfab"
	open.pressed.connect(func() -> void: _request({"action": "open", "uid": uid}))
	buttons.add_child(open)

	return card


# The cached JPEG Bhippi wrote, decoded once. Returns null when there is no thumbnail yet,
# which draws an empty tile rather than an error.
func _thumbnail(path: String) -> Texture2D:
	if path == "" or not FileAccess.file_exists(path):
		return null
	var stamp := FileAccess.get_modified_time(path)
	var cached = _texture_cache.get(path, null)
	if cached != null and int(cached.get("stamp", -1)) == stamp:
		return cached.get("texture", null)
	var image := Image.new()
	if image.load(path) != OK:
		return null
	var texture := ImageTexture.create_from_image(image)
	_texture_cache[path] = {"stamp": stamp, "texture": texture}
	return texture


func _thousands(value: int) -> String:
	if value >= 1000000:
		return "%.1fM" % (float(value) / 1000000.0)
	if value >= 1000:
		return "%.1fk" % (float(value) / 1000.0)
	return str(value)


# ── asking Bhippi for something ──────────────────────────────────────────────────────

func _on_search_submitted(text: String) -> void:
	var query := text.strip_edges()
	if query == "":
		return
	_status_label.text = "Searching…"
	_request({
		"action": "search",
		"query": query,
		"shippable_only": _shippable_check.button_pressed,
		"animated_only": _animated_check.button_pressed,
	})


func _on_connect_pressed() -> void:
	if _connect_button.text == "Sign out":
		_request({"action": "disconnect"})
	else:
		_status_label.text = "Opening your browser…"
		_request({"action": "connect"})


# One request file, written whole. Bhippi takes it and deletes it, so a click that lands
# while Bhippi is busy with the previous one replaces it rather than queueing — which is
# what a person clicking twice actually means.
func _request(payload: Dictionary) -> void:
	var directory := ProjectSettings.globalize_path("res://").path_join(REQUEST_REL).get_base_dir()
	DirAccess.make_dir_recursive_absolute(directory)
	var path := ProjectSettings.globalize_path("res://").path_join(REQUEST_REL)
	var file := FileAccess.open(path, FileAccess.WRITE)
	if file == null:
		print("%s could not write %s — is the project folder writable?" % [LOG_PREFIX, REQUEST_REL])
		return
	file.store_string(JSON.stringify(payload))
	file.close()


# ── the file ─────────────────────────────────────────────────────────────────────────

# The state, or an empty dictionary for "there is nothing to draw". Every failure is that:
# Bhippi writes the file whole by an atomic rename, so a read that fails is a file that is
# not there yet, and the next tick reads it. Nothing here reports into the editor's log,
# which would otherwise fill with a line every 400 ms.
func _read_state() -> Dictionary:
	var path := ProjectSettings.globalize_path("res://").path_join(STATE_REL)
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
	if int(parsed.get("version", 0)) != CHANNEL_VERSION:
		# A newer Bhippi writing a shape this addon does not know. Doing nothing is right:
		# the addon is replaced on the next workspace open, and a guess would be worse.
		return {}
	return parsed
