extends Control

## An on-screen analogue stick. Drawn rather than textured, so it needs no imported art.
## Read `value` from a player script, or let it drive input actions directly.

@export var radius: float = 110.0
@export var knob_radius: float = 44.0
@export var dead_zone: float = 0.2

## -1..1 on both axes, dead zone already applied.
var value: Vector2 = Vector2.ZERO

var _touch_index: int = -1
var _knob: Vector2 = Vector2.ZERO


func _ready() -> void:
	custom_minimum_size = Vector2(radius * 2.0, radius * 2.0)


func _draw() -> void:
	var centre := Vector2(radius, radius)
	draw_circle(centre, radius, Color(1, 1, 1, 0.10))
	draw_arc(centre, radius, 0.0, TAU, 48, Color(1, 1, 1, 0.35), 2.0)
	draw_circle(centre + _knob, knob_radius, Color(1, 1, 1, 0.55))


func _gui_input(event: InputEvent) -> void:
	if event is InputEventScreenTouch:
		if event.pressed:
			_touch_index = event.index
			_move_to(event.position)
		elif event.index == _touch_index:
			_release()
	elif event is InputEventScreenDrag and event.index == _touch_index:
		_move_to(event.position)
	elif event is InputEventMouseButton:
		if event.pressed:
			_touch_index = 0
			_move_to(event.position)
		else:
			_release()
	elif event is InputEventMouseMotion and _touch_index == 0:
		_move_to(event.position)


func _move_to(local: Vector2) -> void:
	var centre := Vector2(radius, radius)
	var offset := local - centre
	if offset.length() > radius:
		offset = offset.normalized() * radius
	_knob = offset
	var raw := offset / radius
	value = Vector2.ZERO if raw.length() < dead_zone else raw
	queue_redraw()


func _release() -> void:
	_touch_index = -1
	_knob = Vector2.ZERO
	value = Vector2.ZERO
	queue_redraw()
