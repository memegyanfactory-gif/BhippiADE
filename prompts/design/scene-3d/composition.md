version: 1
domain: scene-3d
title: Composition
when: one focal point, silhouettes, layers, reading order
tags: composition, focal, silhouette, value, contrast, foreground, midground, background, depth, thirds, leading-line, landmark, negative-space, scale-cue, readability, 3d, scene, level

# Composition

A level is composed from the **gameplay camera**, not from the editor's free camera. Every
rule here is checked by taking the screenshot the player would see.

<!-- section: focal -->
## 1. One focal point per view

At any point the player can stand, one thing should draw the eye: the exit, the landmark,
the threat, the pickup cluster. Make it the brightest or most saturated thing in frame, put
it near a third, give it the only light of its kind, or the only movement. Two focal points
is none; a view with none is a place where players wander.

<!-- section: silhouettes -->
## 2. Silhouettes and value grouping

Gameplay objects read by silhouette first: a pickup, an enemy, a door must be recognisable
as a black shape against a white ground. Then by **value** (lightness): the player and
interactables are lighter or darker than their surroundings by at least two steps of the
neutral ramp; the set dressing sits in the middle values so it never competes. Hue is the
third channel, reserved for the semantic colours (pickup, hazard, goal). A scene where
everything is mid-value and mid-saturation is mud, whatever the models cost.

<!-- section: layers -->
## 3. Foreground, midground, background

Three depth layers with distinct value and detail: foreground darkest and most detailed
(framing elements — a pillar, a branch), midground where play happens (the clearest value
contrast), background lightest and simplest (fog, sky, distant silhouettes). Fog or a
distance colour shift is the cheapest way to make the layers exist
(`scene-3d/lighting-environment#fog`). A level with no background layer feels like a box; a
level with no foreground feels like a diorama.

<!-- section: lines-and-thirds -->
## 4. Leading lines and thirds

Paths, walls, rows of props and light shafts are lines; aim them at the focal point. Put
the horizon on a third, not the middle, and the focal point on an intersection of thirds
from the *default camera position* for that area. In a top-down or isometric game the same
holds for the playfield's landmarks.

<!-- section: negative-space -->
## 5. Negative space

Density is contrast: a cluttered market reads because the square before it is empty. Leave
breathing room around the focal point and around every interactable (`≥ 1.5 m` of clear
floor around a pickup or a door). Set dressing goes in clusters of three to five with empty
ground between clusters, never an even sprinkle.

<!-- section: scale-cues -->
## 6. Scale cues

Something of known size in every view — a door, a chair, a fence, a character-height object
— or the player cannot judge a jump or a fall. Doors are 2.1 m, steps 0.17 m, railings 1.0 m,
tables 0.75 m. A cliff without a tree reads as any height; a corridor without a door has no
size.

<!-- section: reading-order -->
## 7. Reading order

From the spawn, the eye should travel: focal point → the path to it → the first threat or
pickup → the periphery. Test by describing the screenshot in one sentence; if the sentence
starts with a wall, the composition is wrong.

<!-- section: checklist -->
## 8. Before calling a view composed

- one focal point, at a third, brightest or most saturated
- interactables separate by silhouette and value
- three depth layers with a fog or value shift
- a scale cue in frame
- clear floor around every interactable
- describable in one sentence
