version: 1
domain: scene-3d
title: Layout metrics
when: the metric grid, player metrics, derived gap and door numbers
tags: layout, metrics, scale, grid, snap, unit, metre, player, height, jump, gap, ledge, door, corridor, platform, blockout, greybox, csg, density, prop, spacing, 3d

# Layout metrics

Every number in a level derives from the player. Guessing a gap is how a level becomes
unplayable; deriving it is how it becomes tunable.

<!-- section: grid -->
## 1. The metric grid

One Godot unit is one metre. Snap layout to **1 m** for structure (floors, walls, platforms),
**0.5 m** for dressing, **0.25 m** only for fine details. Origins of props at their base
(`scene-3d/model-selection#fit-check`); walls and platforms with their pivot at a corner or
base-centre so snapping stays predictable. Rotate structure in 90° steps, dressing in 15°
steps, and break the rule deliberately for one thing per area (a fallen pillar, a leaning
sign) — regularity everywhere is a prison, regularity nowhere is a landslide.

<!-- section: player-metrics -->
## 2. Player metrics per archetype

Read these from the player preset's exported variables before laying anything out; never
assume them.

| Archetype | Height | Radius | Walk / run m·s⁻¹ | Jump height | Jump distance (run) | Double jump |
|---|---|---|---|---|---|---|
| platformer 3D | 1.8 | 0.4 | 5 / 8 | 2.2 | 6 | +1.6 height |
| third-person action | 1.8 | 0.4 | 4 / 7 | 1.2 | 3.5 | — |
| FPS | 1.8 (eye 1.6) | 0.4 | 5 / 8 | 1.1 | 3 | — |
| top-down 3D | 1.8 | 0.4 | 5 | — | — | — |
| kart | 1.2 × 2.4 | — | 15–30 | — | — | — |
| endless runner | 1.8 | 0.4 | 8 → 16 | 2.0 | lane-based | — |

Derive from the preset: `jump_height = v² / (2g)`, `jump_distance = run_speed × 2v / g`,
with `g` from the project's physics setting. If the preset changes, the level's derived
numbers change with it; the level is authored in terms of the player.

<!-- section: derived -->
## 3. Derived numbers

| Thing | Rule | Platformer 3D value |
|---|---|---|
| easy gap | 0.6 × jump distance | 3.5 m |
| standard gap | 0.8 × jump distance | 4.8 m |
| hard gap | 0.95 × jump distance, never 1.0 | 5.7 m |
| double-jump gap | 0.8 × (jump distance + double reach) | 8 m |
| climbable ledge | ≤ 0.9 × jump height | 2.0 m |
| unclimbable wall | ≥ 1.3 × jump height (with double jump: of the combined height) | 3.0 m |
| door | 2.1 m tall × 1.0 m wide; 1.2 m wide if two can pass | — |
| corridor | ≥ 3 × radius wide (1.2 m), 2.4 m tall; comfortable at 2 m wide | — |
| combat room | ≥ 8 × 8 m for melee; 15 × 15 m for ranged, with cover every 4 m | — |
| step | 0.17 m rise, 0.28 m tread; a slope ≤ 35° walkable | — |
| landmark spacing | one every 30–50 m of path, visible from the previous | — |
| pickup spacing along a path | every 3–6 m in a trail, clusters of 3–5 at rewards | — |
| fall that hurts / kills | > 2 × / > 4 × jump height, stated in the level, never implicit | 4.4 m / 8.8 m |

For a 2D platformer the same rules apply in tiles (`scene-2d/sprites-tiles#metrics`).

<!-- section: blockout -->
## 4. Blockout before dressing

A level is built twice: first as CSG boxes and ramps in the neutral material with the
derived numbers, played until the route, the timing and the sight lines work; then dressed
by replacing blockout volumes with models of the **same bounds**. The blockout is the
level's spec; dressing never changes a gameplay dimension. A blockout volume that survives
into the shipped game is a `todo` tag, not a fault — the model-selection fallback is a
blockout on purpose.

<!-- section: density -->
## 5. Prop density

Per 10 × 10 m of playable area: 2–4 large props (a cart, a tree, a crate stack), 4–8 medium,
8–16 small, arranged in clusters of 3–5 with empty ground between clusters. Clusters near
walls and corners, not in the middle of paths. A cluster contains one hero prop and its
supporting cast (a barrel, three crates, a lantern), never five of the same thing in a row.
Ground clutter (rocks, grass tufts) at 1 per 2 m² in natural areas, near zero on paths.

<!-- section: collision -->
## 6. Collision that matches what is seen

A collider is the visible shape simplified, never bigger: a crate is a box, a tree is a
capsule around the trunk only (the player walks under the canopy), a rock is a convex hull.
Invisible walls are a design failure; use a visible boundary (a fence, a cliff, water) at the
same place. Colliders on dressing are static bodies on the world layer; pickups are areas.

<!-- section: checklist -->
## 7. Before calling a layout done

- every gap, ledge and wall derives from the player metrics table
- snapped to the grid; one deliberate exception per area
- a landmark visible from every path segment
- clear floor around interactables; clusters not sprinkles
- colliders match visible shapes; no invisible walls
- a playtest sample shows the route completes and the hard gap is cleared at run speed
