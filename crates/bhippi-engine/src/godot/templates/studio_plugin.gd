@tool
# Bhippi Studio — the editor, with the docks out of the way.
#
# Why this exists: Bhippi's studio viewport *is* this Godot editor window, re-parented into
# Bhippi's own window (ADR-0045). Stock Godot fills that window with the Scene and Import
# docks on the left, the Inspector and Node docks on the right, and FileSystem below them —
# so the hole the studio reserved for the game shows mostly panels. This plugin turns on
# Godot's own distraction-free mode once, when the editor loads it, which hides every dock
# and the bottom panel and leaves the viewport, its toolbar, the menu bar and the scene tabs.
#
# How to get the docks back: press Ctrl+Shift+F12, or use the distraction-free toggle at the
# top-right of the main screen. Nothing here re-asserts the mode afterwards — no timer, no
# polling — so once you open the docks they stay open for the rest of the session. To stop
# Bhippi hiding them at startup at all, untick "Bhippi Studio" in Project Settings → Plugins.
#
# Written by Bhippi's scaffold (bhippi-engine::godot::scaffold), not by the agent: this is
# the same class of file as bhippi/probe.gd, so INV-088 does not apply to it.
extends EditorPlugin


func _enter_tree() -> void:
	# Deferred by exactly one idle frame, and never repeated. Godot initialises plugins
	# during its filesystem scan, before it restores the saved editor layout ("Loading
	# docks...", "Loading central editor layout..."), so a value written inline here is set
	# while the thing it controls is still being rebuilt. One deferred call lands after that
	# and then this plugin is silent for the rest of the session — no timer, no polling.
	_hide_the_docks.call_deferred()


func _hide_the_docks() -> void:
	EditorInterface.set_distraction_free_mode(true)
