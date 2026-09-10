version: 1
domain: scene-3d
title: Level flow
when: pacing, gating, guidance, sight lines, safe zones
tags: flow, pacing, gating, guidance, sight-line, landmark, wayfinding, checkpoint, safe-zone, difficulty, curve, loop, hub, teach, level-design

# Level flow

Layout is what a level *is*; flow is what the player *experiences over time*.

<!-- section: teach-test-twist -->
## 1. Teach, test, twist

Every mechanic enters in a safe space where failure costs nothing (teach), is required
under mild pressure (test), then combines with something known (twist). A level introduces
at most one new idea. The first thirty seconds of a game teach movement with no threat in
frame.

<!-- section: pacing -->
## 2. Pacing as a curve

Intensity alternates: a calm stretch (traversal, a vista, a pickup trail) before every peak
(a combat room, a hard jump sequence, a boss). Peaks grow across the level; the last calm
stretch before the finale is the longest. Draw the curve before building: `calm 30 s → peak
20 s → calm 20 s → peak 40 s → calm 40 s → finale 60 s`. A level that is all peak numbs; all
calm bores.

<!-- section: guidance -->
## 3. Guidance without arrows

Players go toward light, toward colour, toward movement, and along lines. Light the path;
paint the goal in the goal colour; put a moving thing (a bird, a flag, a waterfall) at the
next landmark; align paths and rows of props toward it. The **weenie** — one tall landmark
visible from most of the level — anchors wayfinding. A pickup trail is the strongest cheap
guide; use it sparingly or it becomes the only thing the player reads. Never a floating
arrow unless the game's world contains arrows.

<!-- section: sight-lines -->
## 4. Sight lines

From every decision point the player sees the next landmark, or the next pickup, or the
next threat — never nothing. Reveal a big space through a narrow entrance (compression and
release). Hide the whole route so it is discovered, but never hide the next step. Block a
sight line on purpose only to stage a reveal.

<!-- section: gating -->
## 5. Gating

A gate is a thing the player cannot pass until they have something: a key, an ability, a
count. Show the gate before its key so the key means something. The gate's visual must say
what opens it (a coloured door for a coloured key, a gap for a double jump) in the same
semantic colour used everywhere else. No invisible gates.

<!-- section: safe-zones -->
## 6. Safe zones and checkpoints

A checkpoint after every peak and before every gate, visible as a landmark in its own right
(a lantern, a flag, a shrine), in the goal colour when reached. A safe zone is readable as
safe: light, open, no enemy spawn, calm music. Respawn faces the direction of travel with
the next objective in frame. A death that costs more than the last peak's worth of progress
is a punishment the player will not accept twice.

<!-- section: loops-and-hubs -->
## 7. Loops and hubs

A path that loops back to a known place teaches the map for free; a hub with spokes lets
the player pick an order. Shortcuts open backward (a gate that unlocks from the far side)
so returning is quick. A dead end holds a reward or it is not a dead end, it is a mistake.

<!-- section: difficulty -->
## 8. Difficulty

Numbers scale with the derived metrics table (`scene-3d/layout-metrics#derived`): easy gaps
early, standard mid, hard late, and never a 1.0 gap. Enemies grow in count before they grow
in kind. An optional hard path beside the main path rewards mastery without gating progress.
Assist options are accessibility and never disable achievements.

<!-- section: checklist -->
## 9. Before calling the flow done

- the pacing curve is drawn and the level matches it
- one new idea, taught before tested
- a landmark or the next step visible from every decision point
- every gate shown before its key; every checkpoint a landmark
- a playtest sample completes the route; the hard gap is at ≤ 0.95
