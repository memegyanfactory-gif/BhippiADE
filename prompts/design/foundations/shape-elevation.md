version: 1
domain: foundations
title: Shape and elevation
when: radius by size, concentric rule, elevation by role
tags: radius, corner, rounded, shadow, elevation, border, card, surface, lift, depth, pill

# Shape and elevation

<!-- section: radius -->
## 1. Radius by size

`4 px` for controls, `8 px` for panels and cards, `6 px` for modals, `999px` for pills. A
radius is *smaller* on smaller elements: 12 px on a 24 px button is a lozenge. Sharp corners
are a legitimate choice for a brutalist, industrial or instrument subject; the choice is
made once per system and held.

<!-- section: concentric -->
## 2. The concentric rule

An inner radius equals the outer radius minus the padding between them. Equal radii on nested
boxes look wrong for a reason people cannot name — the inner corner appears to bulge.

<!-- section: elevation -->
## 3. Elevation, by role

| Level | Use |
|---|---|
| flat | anything in the layout — hairline only |
| lift-1 | hover on a card, a sticky bar |
| lift-2 | menus, drop-ups, popovers |
| lift-3 | modals |

Shadows are for things that float *over* the page. Light themes soften all three levels
together; a dark-theme shadow on a light ground reads as dirt. Shadow colour takes the hue of
the ground, never pure black.

<!-- section: not-everything-is-a-card -->
## 4. Not everything is a card

Border, fill, radius and shadow each say "separate object". One radius and one shadow stamped
on every block flattens the hierarchy; it is the single most recognisable generated look. A
list of things that are only rows is a table; a group of related fields is a fieldset with a
heading, not three cards. Lead with big-number tiles only when those figures are the point of
the page.
