version: 1
domain: scene-3d
title: Camera
when: FOV by perspective, follow, framing, dead zones
tags: camera, fov, follow, chase, third-person, first-person, top-down, isometric, orbit, framing, dead-zone, damping, collision, springarm, shake, look-ahead, 3d

# Camera

The camera is the player's eye and the designer's frame. A level composed for the wrong
camera is composed for nobody.

<!-- section: fov -->
## 1. FOV by perspective

| Perspective | Vertical FOV | Notes |
|---|---|---|
| first person | 70–80° (90 horizontal at 16:9) | lower narrows and nauseates; a slider in Settings |
| third person action | 55–65° | wider makes the character small |
| third person platformer | 60–70° | see the landing zone |
| top-down 3D | 40–50° or orthographic | orthographic for tactics, perspective for feel |
| isometric | orthographic, `size` 12–20 m | 30° / 45° pitch, 45° yaw |
| kart / racer | 60–75°, widens with speed by up to 10° | the speed cue |
| orbit (viewer, builder) | 45–55° | — |

Godot's `Camera3D.fov` is vertical. Never change FOV for effect except the speed widen.

<!-- section: follow -->
## 2. Follow and damping

A follow camera is a `SpringArm3D` (length 4–6 m for third person, 8–12 m for top-down) with
collision on the world layer, a pivot `Node3D` at the character's chest height (1.2 m), and
**damped** tracking: position lerp weight 6–10 per second (`lerp(pos, target, 1 -
exp(-8 * delta))`), rotation slower than position. Vertical follow is lazier than horizontal
(a platformer camera does not chase every jump; it moves when the player lands, or when they
fall past a threshold). A camera that snaps is the most common cause of "it feels cheap".

<!-- section: framing -->
## 3. Framing the player

The player sits at a third — lower third for a platformer (see what is ahead and above), left
or right third in a side-facing action game, centre only for first person and top-down.
**Look-ahead**: offset the target in the direction of travel by 1–2 m (more at speed) so the
player sees where they are going, not where they are. A dead zone of 0.5–1 m around the
target where small movements do not move the camera stops the jitter.

<!-- section: collision -->
## 4. Collision and occlusion

The spring arm shortens on world collision with a margin of 0.3 m; the character fades or
dithers out under 1 m; the camera never clips through a wall and never leaves the player
hidden behind one for more than a frame. Thin props (poles, fences) are excluded from the
camera's collision mask.

<!-- section: shake -->
## 5. Shake and effects

Shake is applied to the camera's offset, never to its position (`h_offset`, `v_offset`), from
the trauma model in `game-ui/feedback-juice#shake`, with a toggle. Head bob in first person
is off by default and tiny when on (0.03 m at walking speed). Motion blur off by default.

<!-- section: top-down -->
## 6. Top-down and isometric specifics

Bounds: the camera stops at the playfield's edge (a `Camera2D`-style limit, implemented as a
clamp on the pivot) so the void is never in frame. Zoom steps, not a continuous zoom, in a
tactics game. The player is always visible: buildings between camera and player fade to
40 % or cut away.

<!-- section: as-design-tool -->
## 7. The camera as a design tool

Fix the camera for a moment to stage a reveal (a `Camera3D` on a `Path3D`, blended in over
`--t-settle`); pull back at a vista; push in at a door. Every fixed-camera moment has a skip
after the first viewing. Compose every level from the default follow position and check
`scene-3d/composition#focal` from it.

<!-- section: checklist -->
## 8. Before calling the camera done

- FOV from the table; a slider in first person
- damped follow with look-ahead and a dead zone; lazy vertical
- spring-arm collision; the player never occluded for more than a frame
- shake on offset with a toggle; no bob by default
- a playtest sample shows the camera never clips and the player stays framed
