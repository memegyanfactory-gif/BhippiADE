version: 1
domain: game-ui
title: HUD
when: what earns screen, safe area, readable at distance, bars, minimap
tags: hud, overlay, health, bar, score, counter, timer, ammo, minimap, reticle, crosshair, damage-number, safe-area, anchor, readable, distance, diegetic, canvaslayer

# HUD

The HUD is the part of the game the player looks at most and sees least. Everything on it
costs attention that belongs to the world.

<!-- section: budget -->
## 1. The HUD budget

Write the budget before the first `Label`: which facts the player needs **every second**
(health, the objective direction, ammo in a shooter), which they need **on change** (a
pickup, a score tick, a wave number), and which they need **on demand** (the map, the
inventory, stats). Only the first group is always visible. The second appears on change and
fades. The third is a screen behind a button. A HUD with more than five persistent elements
has a budget problem, not a layout problem. Archetype defaults: platformer — lives or
checkpoint marker, collectible count; racer — position, lap, speed; shooter — health, ammo,
reticle; survival — three meters and a compass; puzzle — moves and the goal; tower defence —
resources, wave, base health.

<!-- section: anchors -->
## 2. Anchors and the safe area

Persistent elements live in the corners and edges; the centre belongs to the game (the
reticle excepted). Top-left: the player's state (health, lives). Top-right: session state
(score, timer, wave). Bottom-left or -right: resources, ammo, abilities. Bottom-centre:
nothing persistent. Every element sits inside a **safe area** inset of 4 % of the shorter
screen edge (TVs overscan; phones have notches); in Godot use `Control` anchors with margins
from a theme constant, never absolute positions, and a `CanvasLayer` so the HUD ignores the
camera (`game-ui/godot-control#hud-layer`).

<!-- section: readable-at-distance -->
## 3. Readable at distance

A HUD is read at two to three metres on a TV and at arm's length on a phone, over a moving,
unpredictable backdrop. Sizes at 1080p: persistent numbers 24–28 px, labels 18–20 px,
subtitles 26–30 px, damage numbers 20–32 px scaled by magnitude; scale by the viewport
(`content_scale_mode`) so 4K does not shrink them. Every text over the world has a **plate**
(a rounded box at 40–60 % of `bg`), an **outline** (1.5–2 px in the darker neutral) or a
**scrim** (a gradient at the screen edge) — bare text over a scene fails the contrast floor
somewhere in every level. Use a face with open counters and a tall x-height; a display face
is for the title screen, not the score.

<!-- section: bars-and-counters -->
## 4. Bars, meters and counters

A bar is a meter (`web/charts#in-games`): one hue on its own ramp, a track one step off the
plate, the value in text beside or inside it, a 2 px gap between segments if segmented.
Damage shows as a delayed "ghost" segment draining after the real one (`--t-settle`), so the
player reads how much was lost. Low state is signalled three ways: the bar's hue shifts, a
glyph or label appears, and the bar (not the screen) pulses at `--t-ambient` — never a
red vignette alone. Counters use tabular numerals and change with a short scale tick
(`--t-quick`); a counter that flips digits like an odometer is a toy.

<!-- section: minimap-compass -->
## 5. Minimap and compass

A minimap is a chart: a fixed categorical legend (player, enemy, objective, pickup) whose
slots never change meaning between levels; north-up unless the game is a racer; a border
that is a plate, not a frame. A compass strip across the top is cheaper than a minimap for an
exploration game and interrupts the world less. Neither is on by default in a puzzle or a
platformer.

<!-- section: reticle -->
## 6. Reticle and interaction prompt

The reticle is the one persistent centre element: small, two-colour (light shape, dark
outline), state by shape — a dot at rest, a ring on a target, a bracket on an interactable —
never by colour alone. The interaction prompt sits under it: the glyph of the *current*
input device (the prompt swaps when the player touches a gamepad) plus a verb, *Open*, *Talk*,
*Pick up*.

<!-- section: diegetic -->
## 7. Diegetic when the subject allows it

A fuel gauge on the dashboard, a health bar on the suit's wrist, ammo on the gun — diegetic
elements cost nothing in attention and belong to the world. Use them when the camera makes
them readable; keep an overlay fallback for accessibility (a diegetic-only HUD fails the
readable-at-distance rule for some players).

<!-- section: never -->
## 8. Never on a HUD

Anything that blinks continuously; a full-screen red flash on damage (a short vignette pulse
of 140 ms with a shake is the ceiling, and it has a toggle); text under 18 px at 1080p; an
element the player cannot explain after five minutes of play; the default Godot theme; a
gradient plate; more than one accent.
