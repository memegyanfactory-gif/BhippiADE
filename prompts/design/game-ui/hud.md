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

<!-- section: categories -->
## 9. The 16 HUD Presets and Categories

Bhippi provides 16 built-in HUD presets tailored to distinct game archetypes:

1. **`preset.hud.health_score`** (Action / Platformer): Health bar top-left, score and lives top-right.
2. **`preset.hud.lives_score`** (Classic 2D / Arcade): Heart pips top-left, score top-right.
3. **`preset.hud.lap_timer`** (Racing / Time Trial): Lap counter, split timer, speedometer ring.
4. **`preset.hud.wave_counter`** (Horde / Tower Defence): Wave badge, enemy counter, base integrity.
5. **`preset.hud.collectible_counter`** (Collect-a-thon): Primary & secondary counters, world timer.
6. **`preset.hud.ammo_health`** (Shooter / Arena FPS): Armor/health bottom-left, ammo reserve & reticle.
7. **`preset.hud.distance_score`** (Endless Runner): Distance meter, multiplier, personal best.
8. **`preset.hud.survival_meters`** (Survival / Open World): Health, hunger & stamina stack, clock, compass.
9. **`preset.hud.move_counter`** (Puzzle): Turn counter and par moves, restart action button.
10. **`preset.hud.boss_fight`** (Boss Arena): Named boss health bar with phase pips across top.
11. **`preset.hud.stealth_awareness`** (Stealth / Infiltration): Curved awareness arc, noise meter, reticle.
12. **`preset.hud.combo_rhythm`** (Rhythm / Hack & Slash): Hit combo counter, accuracy bar, beat indicator.
13. **`preset.hud.explore_map`** (Adventure / Exploration): Rotating/North-up circular minimap with radar blips.
14. **`preset.hud.scifi_mecha`** (Sci-Fi / Mecha Combat): Armor & shield bars, heat gauge ring, level badge.
15. **`preset.hud.hero_moba`** (Hero Action / MOBA): Character portrait & level, HP/MP bars, 4-slot ability cooldowns.
16. **`preset.hud.pixel_rpg`** (2D Retro / Pixel RPG): Pixel heart containers, stamina bar, rupee/key counters, item grid.
17. **`preset.hud.sim_cockpit`** (Simulator / Vehicle): Fuel meter, gear readout, circular speed dial.
18. **`preset.hud.fighting_combo`** (Fighting / Dual Duel): P1 & P2 health bars, super meter, hit counter.
19. **`preset.hud.retro_arcade`** (Retro Shmup / Arcade): 1UP / High-Score counters, credits, bomb pips, life pips.
20. **`preset.hud.minimal`** (Atmospheric / Narrative): Contextual objective text and quiet toast notifications.

<!-- section: local-icons -->
## 10. Built-in Local Icon & Widget System

The studio provides 24 standard icon roles with zero external dependencies, available in both
**Clean Vector** (sleek modern geometry) and **Pixel Art** (16x16 crisp-edge silhouettes):
- `heart`, `shield`, `mana`, `stamina`, `coin`, `gem`, `star`, `clock`, `ammo`, `bolt`, `food`,
  `eye`, `key`, `sword`, `potion`, `fuel`, `speed`, `skull`, `bomb`, `compass`, `trophy`,
  `target`, `badge`, `diamond`.

Specialized HUD widgets ready for AI instantiation:
- **`stealth_arc`**: Dynamic curved awareness indicator with 4 states (`calm`, `alert`, `visible`, `danger`) and central detection chevron.
- **`portrait`**: Character avatar frame with color-accent border, player title, and level badge.
- **`item_grid`**: 2x3 inventory or equipment matrix with cooldown overlay sweeps and stack counters.
- **`ring`**: Radial percentage meter for speed, heat, boost, or cooldown timers.
- **`compass`**: Horizon cardinal tape heading ribbon (`N`, `E`, `S`, `W`).
- **`minimap`**: Radar circle displaying categorical blip pings (`player`, `enemy`, `objective`, `pickup`).

<!-- section: runtime-api -->
## 11. HUD Runtime GDScript API

When the game logic runs, communicate with the HUD strictly through its root script methods:
```gdscript
# Update values and maxima (ghosts, ticks, low-state pulses update automatically)
hud.set_value("player.health", current_hp)
hud.set_max("player.max_health", max_hp)

# Stealth arc state ("calm", "alert", "visible", "danger")
hud.set_stealth_state("alert")

# Minimap blips relative to player (offset in meters, role string)
hud.set_map_targets([
    {"offset": Vector2(12.0, -8.0), "role": "enemy"},
    {"offset": Vector2(-5.0, 20.0), "role": "objective"},
])

# Reticle state ("rest", "target", "interact", optional prompt verb)
hud.set_reticle_state("interact", "Examine")

# Toast notifications
hud.notify("Checkpoint Reached!")
```

<!-- section: font-doctrine -->
## 12. Curated Game Font Doctrine

Typography establishes genre conviction before a single mechanic is learned. Bhippi bundles
5 open-source SIL OFL-1.1 typefaces curated specifically for legible, high-contrast game HUDs:

1. **Orbitron** (`res://assets/fonts/Orbitron-Bold.ttf`): Geometric cyberpunk / mecha display face with high-contrast angular counters. Paired with `fps_arena`, `sci-fi`, and `racer` archetypes.
2. **Rajdhani** (`res://assets/fonts/Rajdhani-Bold.ttf`): Condensed technical sans-serif with square shoulders. Ideal for high-density diagnostics, tactical dashboards, and flight avionics.
3. **Press Start 2P** (`res://assets/fonts/PressStart2P-Regular.ttf`): True 8-bit bitmap pixel font. Paired with `pixel_rpg`, retro arcade shmups, and classic platformers.
4. **Cinzel** (`res://assets/fonts/Cinzel-Bold.ttf`): Roman inscriptional serif with neoclassical proportions. Paired with fantasy RPGs, epic boss encounters, and card battlers.
5. **Outfit** (`res://assets/fonts/Outfit-Bold.ttf`): Clean geometric humanist sans-serif. Highly readable at all scales, chosen for casual puzzle games, mobile arena HUDs, and runners.

When a project HUD is built, `fonts::font_for_archetype(archetype, skin_id)` automatically pairs the optimal typeface, copies the `.ttf` into `assets/fonts/`, and attaches the required legal `.meta.json` sidecar. The generated HUD script applies this font across all labels, counters, and buttons via `add_theme_font_override("font", _custom_font)`.

<!-- section: procedural-panels -->
## 13. Procedural Panels, Boxes & 9-Slice Styling

HUD panels provide the contrast plate separating readable UI from 3D world geometry. Bhippi generates 5 procedural SVG 9-slice frame textures and matching interactive button states:

1. **`SciFiWireframe`**: 45° chamfered cut corners, outer glowing perimeter frame, corner bracket accents, and subtle technical tick marks (inspired by vehicle telemetry and Freepik car diagnostics).
2. **`HeroHex`**: 12-sided faceted polygon border with diagonal hazard corner brackets, slanted energy stripes, and hexagonal portrait masks (inspired by Overwatch and hero shooters).
3. **`CasualGlossy`**: Multi-layer embossed pill cards with top-half specular highlight sheen, thick golden borders, and saturated depth drop shadows (inspired by Empire City and match-3 quest trees).
4. **`BrawlPill`**: Bold cartoon dark-stroke outlines, rounded pill geometry, energetic bright header plates, and bottom isometric shadow bases (inspired by Brawl Stars and mobile arenas).
5. **`RetroPixel`**: Stepped 3-tone bevel framing (highlight, face, shadow) with 2px corner drop-in and zero blur for authentic 16-bit retro vibes.

Each panel style automatically generates:
- `panel_<skin>.svg`: 9-slice plate texture with 12px safe margins.
- `button_<skin>_normal.svg`: Interactive button plate in resting state.
- `button_<skin>_pressed.svg`: Depressed button plate with shifted inner bevel and active highlight.
- `assets/ui/panels/<file>.meta.json`: CC0-1.0 license attributions for game distribution.

<!-- section: ai-generator -->
## 14. Autonomous AI UI Generation Pipeline

When an agent needs a HUD icon, panel, or button that does not currently exist in the local library:
1. **Procedural Synthesis**: Call `godot::hud::generator::synthesize_icon_svg(role, skin_id, color)` or `synthesize_box_svg(kind, color, accent)`. The generator parses the semantic role (`laser`, `chest`, `battery`, `portal`, `crown`, etc.) and produces vector geometry mathematically.
2. **Heraldic Novel Fallback**: For completely unknown roles, the generator produces a unique geometric heraldic crest badge with a distinct glyph pattern.
3. **Automatic Registration**: Call `register_and_install_custom_icon(project_root, role, skin_id, color)`. This writes the `.svg` into `assets/ui/icons/` and generates an INV-074 compliant `.meta.json` sidecar naming the author and CC0 license.
4. **Library Self-Improvement**: By registering newly synthesized assets into the project's asset catalogue, the HUD library continuously expands its vocabulary for subsequent builds.

