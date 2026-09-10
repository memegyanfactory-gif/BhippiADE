version: 1
domain: game-ui
title: Godot Control practice
when: anchors, containers, Theme as tokens, focus, scaling
tags: godot, control, theme, anchor, container, margin, hbox, vbox, grid, label, button, panel, stylebox, font, msdf, focus, neighbour, canvaslayer, scale, viewport, tres, override

# Godot Control practice

How the design tokens become Godot nodes, through typed actions only. Every rule here lowers
to `add_node`, `set_property`, `write_script` and a `Theme` resource; nothing here is a
hand-written scene.

<!-- section: theme-as-tokens -->
## 1. The `Theme` resource is the token layer

One `Theme` (`res://ui/theme.tres`) holds every colour, font, font size, constant and
`StyleBox` for the game, set on the root `Control` (or the project's `gui/theme/custom`)
so every child inherits. The plan's tokens map directly:

| Token | Theme item |
|---|---|
| `--bg`, `--surface`, `--surface-2` | `StyleBoxFlat` `bg_color` on `Panel`, `PanelContainer`, `Button` normal / hover / pressed |
| `--line` | `StyleBoxFlat` `border_color`, `border_width_*` = 1 |
| `--text`, `--text-dim` | `Label/colors/font_color`, `Button/colors/font_color`, `font_disabled_color` |
| `--accent`, `--on-accent` | `Button/styles/focus` and the primary button's `StyleBoxFlat`, `font_focus_color` |
| `--radius` | `corner_radius_*` on the StyleBoxes (4 controls, 8 panels) |
| spacing `4 · 8 · 16` | `HBoxContainer/constants/separation`, `MarginContainer/constants/margin_*` |
| `--fs-*` | `default_font_size`, `Label/font_sizes/font_size`, a `HeaderLarge` type variation |

`theme_override_*` on a node is the **exception**, used for a documented one-off (the one
danger button, the hero title) and never as the way to style a screen. A screen styled by
overrides is a screen nobody can re-theme.

<!-- section: anchors-containers -->
## 2. Anchors and containers over positions

Never set `position` on a `Control` for layout. A screen is a tree of containers:
`MarginContainer` (the safe-area inset) → `VBoxContainer` / `HBoxContainer` (rows and
columns, `separation` from the grid) → `PanelContainer` (a surface) → leaf controls. Use
`size_flags_horizontal = SIZE_EXPAND_FILL` to share space, `custom_minimum_size` to hold a
floor, `GridContainer` for a real grid, `CenterContainer` for the one centred thing. Corner
HUD elements use full-rect anchors on the layer plus a `MarginContainer` per corner, or
anchor presets (`PRESET_TOP_LEFT` …) with the margin from a theme constant — so the safe
area is one number.

<!-- section: hud-layer -->
## 3. The HUD layer

The HUD is a `CanvasLayer` (layer 10) with a full-rect `Control` root named `HUD`; menus
are a higher layer (20) so they cover it; a debug layer sits above both. The HUD root has
`mouse_filter = MOUSE_FILTER_IGNORE` so it never eats gameplay input; menus set
`MOUSE_FILTER_STOP` on their scrim. Pause menus set `process_mode = PROCESS_MODE_WHEN_PAUSED`
and the game tree pauses under them.

<!-- section: fonts -->
## 4. Fonts

A `.ttf` / `.otf` in `assets/fonts/` with a licence sidecar (OFL for most Google faces; the
sidecar carries the text verbatim), imported as a `FontFile` with **MSDF** on for anything
that scales (HUD, titles) and hinting on for small body text; `antialiasing` LCD off (it
breaks on transparent plates). Set fonts on the Theme (`default_font`, a `Title` type
variation), never per Label. Use `font_variation` for a variable face's weight. Subpixel
positioning on for text that moves. An unlicensed font is an `unknown` asset and blocks
Release.

<!-- section: focus -->
## 5. Focus

Every focusable control has `focus_mode = FOCUS_ALL` and explicit `focus_neighbor_*`
NodePaths (containers set them for you only in the trivial case); the first control grabs
focus on screen enter (`grab_focus()` in `_ready`, after one frame); the Theme's
`Button/styles/focus` StyleBox is the unmistakable state (a plate plus the accent border, a
1.02 scale tick on focus via a tween). `ui_cancel` is mapped on every menu to *back*. Test
with the mouse unplugged.

<!-- section: scaling -->
## 6. Resolution and scale

Project settings: `display/window/stretch/mode = canvas_items`, `aspect = expand`, a base
size of 1920 × 1080, `content_scale_factor` driven by the UI-scale setting. Design at 1080p;
HUD elements keep their anchors so wider screens gain world, not stretched UI; taller screens
gain vertical safe area. Never `viewport` stretch for a UI-heavy game (it blurs text).

<!-- section: labels-and-plates -->
## 7. Labels over the world

A HUD `Label` sits inside a `PanelContainer` whose StyleBox is the plate (40–60 % alpha of
`bg`, radius 4, `content_margin` 4 / 8) — or carries `font_outline_size = 2` with
`font_outline_color` from the darker neutral. Subtitles are a `RichTextLabel` on a scrim at
the bottom safe margin, `autowrap`, centred, `fit_content`. Damage numbers are a pooled
`Label3D` (billboard, `no_depth_test`, MSDF) tweened up and out over `--t-settle`.

<!-- section: batch-shape -->
## 8. The batch

One screen is one batch: `create_scene` for the screen (`Control` root), the container tree
with `add_node`, properties with `set_property` (anchors and size flags included), the Theme
attached through the typed resource path, the script through `write_script` (focus, signals,
`ui_cancel`), `connect_signal` for buttons. The label is what the user sees on Undo:
*"Pause menu with settings and quit"*.
