version: 1
domain: game-ui
title: Menus and flow
when: title to play in two inputs, pause, settings, results, focus
tags: menu, title, main-menu, pause, settings, options, results, game-over, dialog, confirm, focus, gamepad, back, flow, screen, loading, splash

# Menus and flow

<!-- section: title -->
## 1. The title screen

From launch to play in **two inputs**: press any key, then *Continue* or *Play*. The title
is the game's name in its display face, sized for the room, over a still or slow moment from
the game's world in its own palette — never the first level with a label on it, never a
system font. One column of focusable rows (Continue · New game · Settings · Credits · Quit),
the first focused, the current one marked by a glyph and a plate, not colour alone. Music
starts here; the version number sits in a corner at `--fs-xs`.

<!-- section: pause -->
## 2. Pause

Instant, from one input, over a dimmed and frozen frame (a scrim at 60 % of `bg`, optional
blur where the platform can afford it). Resume is first and focused; Settings and Quit follow;
Quit confirms. Back or the pause key resumes. The world's audio ducks; the menu's does not
replace it with silence.

<!-- section: settings -->
## 3. Settings anatomy

Tabs or sections, in this order: **Audio** (master, music, effects, voice — sliders with the
value in text), **Video** (display mode, resolution, vsync, frame cap, quality preset, UI
scale), **Controls** (remap per action per device, a reset), **Accessibility** (subtitles,
subtitle size, colour-blind mode, screen shake, flashes, camera bob, hold-to-toggle,
game-speed assist), **Game** (difficulty, language, HUD elements). Every control is a row:
label left, control right, one per line, 44 px tall on touch. Changes apply live where safe
and confirm where not (resolution, with a countdown revert). Defaults are one button away.

<!-- section: results -->
## 4. Results, win, lose

The result in one word in the display face, then the numbers that mattered (time, score,
collectibles found of total) in tabular numerals with a staggered reveal (34 ms), then the
actions: **Retry** first and focused after a loss, **Next** first after a win, *Menu* last. A
loss screen is terse and fast to leave; a win screen may take one motion moment. A graph
(lap times, waves survived) follows `web/charts#in-games`.

<!-- section: dialogs -->
## 5. Confirms and dialogs

A dialog for a consequence only: quit without saving, delete a save, reset settings. Title,
one sentence, two actions named for their effect (*Delete save* / *Keep*), the safe one
focused, Back cancels. Never a dialog for information a toast could carry, never a dialog
with three buttons.

<!-- section: focus -->
## 6. Focus and input

Every menu control has focus neighbours set for up/down (and left/right for sliders and
tabs); the focused control is unmistakable — a plate, a glyph, a size lift, and a sound.
Back always works and always goes one level up. The prompt glyphs swap with the active
device. The mouse works too, and hovering moves focus. See
`game-ui/godot-control#focus`.

<!-- section: loading -->
## 7. Loading

A loading screen shows progress as a bar with a percentage, a tip or a piece of the world,
and never a spinner alone; under two seconds, skip the screen and fade. Never block input on
a screen the player has already seen ten times.

<!-- section: flow -->
## 8. The flow, as a graph

`Title → (New | Continue) → Level → (Pause ↔ Level) → Result → (Next | Retry | Title)`, with
Settings reachable from Title and Pause, and Credits from Title. Every arrow has a back
arrow. A screen the player can be stuck on is a bug.
