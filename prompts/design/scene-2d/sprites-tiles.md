version: 1
domain: scene-2d
title: Sprites and tiles
when: pixel grid, filtering, seams, parallax, silhouettes
tags: 2d, sprite, pixel, pixel-art, tile, tilemap, tileset, parallax, camera2d, silhouette, outline, filter, nearest, seam, vector, animation, frame, resolution

# Sprites and tiles

<!-- section: pixel-grid -->
## 1. The pixel grid

Pick the game's pixel size once — a 16 × 16 or 32 × 32 tile at a base resolution of
320 × 180 or 640 × 360 — and hold it everywhere: every sprite, every tile, every UI glyph at
the same texel density, the camera snapped to whole pixels
(`Camera2D.position_smoothing` with `snap_2d_transforms_to_pixel` on), the viewport at an
integer scale (`stretch/mode = viewport`, `aspect = keep`, integer scaling on). Mixed
densities (a 16 px character on a 64 px tile) are the most recognisable amateur look.
Rotation of pixel sprites only in 90° steps; scale only by integers.

<!-- section: filtering -->
## 2. Filtering per style

Pixel art: `texture_filter = NEAREST` on the project default, no mipmaps, no antialiasing on
lines. Vector and painterly: `LINEAR_WITH_MIPMAPS`, sprites authored at 2× the display size
for crisp downscale. Never nearest on a smooth style or linear on a pixel style; the choice
is one project setting, not per sprite.

<!-- section: silhouettes -->
## 3. Silhouettes and outlines

A sprite reads as a black shape first: the player is the most distinct silhouette in the
game; enemies differ from each other by silhouette, not palette swap alone. An outline
(1 px in the darkest neutral, or a selective outline on the player only) separates sprites
from busy backgrounds; if used, it is used on every gameplay sprite and on no background
tile. Value contrast: gameplay sprites two ramp steps from the background's average value.

<!-- section: palette -->
## 4. Palette

A fixed palette of 16–32 colours for pixel styles, from the brief, shared by every asset
(`art-direction/styles#pixel-16`); ramps of 4–5 values per hue with a hue shift toward warm
in the lights and cool in the shadows. Backgrounds use the desaturated, mid-value part of
the palette; gameplay uses the saturated part; the semantic three are unique hues no tile
uses.

<!-- section: tiles -->
## 5. Tiles and seams

Tile edges match by construction (author in a tileset with the seam checked at 1 px);
`TileMapLayer` with the texture padded (`use_texture_padding` on) so seams never bleed at
non-integer zoom. Terrain autotiles for ground, manual tiles for dressing. Break repetition
every 6–10 tiles with a variant. Collision shapes on the tile set, matching the drawn shape
(a slope is a slope, not a stair of boxes).

<!-- section: parallax -->
## 6. Parallax

Three to five layers with scroll ratios `0.0 (sky) · 0.2 · 0.5 · 0.8 · 1.0 (play)` and a
foreground at `1.2` if the style allows; each layer lighter and less saturated the further
back (`scene-3d/composition#layers`, in 2D). Layers repeat horizontally with a mirror-safe
edge. Never move the sky.

<!-- section: camera -->
## 7. Camera

`Camera2D` with `limit_*` set to the level bounds so the void is never seen, a look-ahead of
2–3 tiles in the direction of travel, position smoothing at 6–10, vertical smoothing lazier
than horizontal, a dead zone (`drag_*_margin`) of 0.1–0.2 for platformers. Screen shake on
the camera `offset` with the trauma model and a toggle.

<!-- section: metrics -->
## 8. Metrics, in tiles

Derive from the player preset as in 3D (`scene-3d/layout-metrics#derived`) but in tiles:
a 16 px player 2 tiles tall jumps 3.5 tiles high and 5 tiles far at run speed; easy gap 3
tiles, hard 4.5, never 5; a climbable ledge 3 tiles, an unclimbable wall 5. Doors 3 tiles;
corridors 3 tall × 3 wide minimum.

<!-- section: animation -->
## 9. Animation

Frame counts by style: pixel — idle 4, walk 6–8, jump 2 + fall 2, attack 4–6, at 8–12 fps;
vector — 12–24 fps with tweened parts. Squash and stretch on takeoff and landing. Every
gameplay sprite has idle, walk and a hit reaction at minimum; a sprite without an idle is a
statue.

<!-- section: ui -->
## 10. UI in a pixel game

The HUD uses the same pixel grid and palette, a pixel font at the base resolution (never a
vector font scaled to fractional pixels), plates from the darkest neutral at full opacity
(alpha dithering only if the style is strict), and 9-patch `StyleBoxTexture` panels drawn in
the tileset. `game-ui/hud` applies with sizes in base pixels: HUD text ≥ 7 px cap height at
320 × 180.
