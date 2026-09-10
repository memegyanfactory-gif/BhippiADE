version: 1
domain: art-direction
title: Style packs
when: twelve coherent style packs and what they never mix with
tags: style, pack, look, low-poly, pixel, flat, vector, painterly, neon, paper, cel, toon, realistic, clay, voxel, ps1, retro, minimal, mono, catalogue

# Style packs

A pack is a coherent set of choices that have been seen to work together. The brief starts
from one and may override any line; model selection reads its asset tags and its never-mix
list; the environment preset is applied as one batch. Section ids are the pack ids.

<!-- section: low-poly-toy -->
## low-poly-toy
- **Mood**: bright, bouncy, friendly. Archetypes: platformer 3D, kart, cosy anything.
- **Palette**: bg `#2b3a55` · ground `#5c8f4a` · mid `#a67c52` · light `#f6e7c8` · accent
  `#f0603c`; warm neutrals.
- **Type**: Baloo 2 800 / Nunito / Nunito 600.
- **Shape**: rounded, chunky, exaggerated proportions (heads and hands big), no thin parts.
- **Materials**: flat vertex colours, roughness 0.8, per-vertex shading optional, no textures.
- **Lighting**: clear noon or golden hour, SSAO low, no glow, saturation 1.1.
- **Asset tags**: `low-poly`, `kenney`, `toy`, `rounded`, `flat-shaded`.
- **Godot**: `LINEAR` filter, MSDF fonts, outlines off.
- **Never mix**: photographic textures, PBR metals, pixel sprites, neon glow.

<!-- section: pixel-16 -->
## pixel-16
- **Mood**: crisp, nostalgic, precise. Archetypes: platformer 2D, top-down 2D, puzzle.
- **Palette**: a 32-colour ramp set; bg `#1a1c2c` · ground `#3b5d3a` · mid `#8c5a3c` ·
  light `#f4f0d8` · accent `#ef7d57`; cool shadows, warm lights.
- **Type**: a pixel font at base resolution (Pixelify Sans for titles at 2×, a 5 × 7 bitmap
  for HUD); never a vector font.
- **Shape**: 16 px tiles, 2-tile characters, 1 px dark outline on gameplay sprites.
- **Materials**: none — sprites; dithering allowed for gradients.
- **Lighting**: painted into the sprites; a `CanvasModulate` for time of day at most.
- **Asset tags**: `pixel`, `16px`, `pixel-art`, `sprite`, `tileset`.
- **Godot**: `NEAREST`, integer scaling, `viewport` stretch, pixel snap.
- **Never mix**: smooth vector sprites, rotated sprites, scaled fonts, mixed tile sizes.

<!-- section: flat-vector -->
## flat-vector
- **Mood**: clean, readable, tidy. Archetypes: top-down action, tower defence, puzzle.
- **Palette**: bg `#eef1f5` (light) · ground `#d9dee6` · mid `#8a94a6` · light `#ffffff` ·
  accent `#2f6fed`; cool neutrals.
- **Type**: Familjen Grotesk 700 / Instrument Sans / Geist Mono.
- **Shape**: geometric, consistent corner radius, silhouettes over detail.
- **Materials**: flat, unshaded or two-tone; roughness 1.0 in 3D.
- **Lighting**: flat or overcast preset, no shadows or hard short shadows, no SSAO.
- **Asset tags**: `flat`, `vector`, `geometric`, `icon-like`, `isometric`.
- **Godot**: `LINEAR_WITH_MIPMAPS`, 2× authored sprites; in 3D `shading_mode UNSHADED`.
- **Never mix**: painterly texture, glow, gradients on gameplay objects.

<!-- section: painterly-soft -->
## painterly-soft
- **Mood**: calm, vast, storybook. Archetypes: exploration, cosy, narrative.
- **Palette**: bg `#3a4d6b` · ground `#7d8a5c` · mid `#b98b5e` · light `#f3e5c6` · accent
  `#d9744c`; warm.
- **Type**: Newsreader 600 / Source Serif 4 / IBM Plex Mono.
- **Shape**: organic, soft edges, asymmetry, foliage in clumps.
- **Materials**: hand-painted or gradient-mapped textures, roughness 1.0, no specular.
- **Lighting**: golden hour or overcast, dense fog, warm key over cool ambient, soft shadows.
- **Asset tags**: `painterly`, `stylized`, `hand-painted`, `soft`, `organic`.
- **Godot**: `LINEAR_WITH_MIPMAPS`, fog on, volumetric optional, saturation 1.0.
- **Never mix**: hard outlines, pixel art, chrome metals, neon.

<!-- section: neon-arcade -->
## neon-arcade
- **Mood**: electric, fast, night. Archetypes: endless runner, racer, arena.
- **Palette**: bg `#0b0b1a` · ground `#161633` · mid `#2a2a5a` · light `#e8e8ff` · accent
  `#ff2d95`, second accent `#2de2ff` (same chroma and lightness).
- **Type**: Orbitron 700 (titles) / Exo 2 / Share Tech Mono.
- **Shape**: hard edges, grids, wireframe accents, long horizontals.
- **Materials**: dark matte bodies with emissive edges, `emission_energy` 2.
- **Lighting**: neon night preset, glow on (0.5), dense fog, low key.
- **Asset tags**: `neon`, `synthwave`, `wireframe`, `cyber`, `glow`.
- **Godot**: glow on, HDR, `LINEAR`, bloom threshold 1.0.
- **Never mix**: daylight presets, pastel, painterly textures, wood.

<!-- section: paper-cut -->
## paper-cut
- **Mood**: crafted, playful, tactile. Archetypes: puzzle, narrative, kids.
- **Palette**: bg `#f7efe3` · ground `#e6d6bd` · mid `#c9a27a` · light `#fffaf2` · accent
  `#d94f3d`; warm paper neutrals.
- **Type**: Gaegu 700 / Atkinson Hyperlegible.
- **Shape**: layered flat planes with slight offsets, torn or cut edges, drop shadows
  between layers only.
- **Materials**: flat with a paper noise texture at low contrast, roughness 1.0.
- **Lighting**: overcast preset with one soft directional for the layer shadows.
- **Asset tags**: `paper`, `cutout`, `craft`, `layered`, `flat`.
- **Godot**: 2D layers with parallax, or 3D planes; `LINEAR_WITH_MIPMAPS`.
- **Never mix**: glow, metals, realistic textures, pixel art.

<!-- section: cel-shaded -->
## cel-shaded
- **Mood**: loud, fast, comic. Archetypes: kart, action, arena.
- **Palette**: bg `#243447` · ground `#6aa84f` · mid `#e0a33a` · light `#fff5d6` · accent
  `#ff4d4d`; saturated.
- **Type**: Bricolage Grotesque 700 / Public Sans / Barlow Condensed.
- **Shape**: bold silhouettes, exaggerated motion, thick consistent outlines.
- **Materials**: toon shader with 2–3 bands, rim light, outline 0.03 m inverted hull.
- **Lighting**: clear noon, hard shadows, no SSAO, saturation 1.15.
- **Asset tags**: `toon`, `cel`, `cartoon`, `outline`, `stylized`.
- **Godot**: toon shader material on everything visible; `LINEAR`.
- **Never mix**: PBR realism, soft painterly, pixel.

<!-- section: gritty-realistic -->
## gritty-realistic
- **Mood**: hard, tense, cold. Archetypes: FPS, survival, horror.
- **Palette**: bg `#0f1114` · ground `#3a3f45` · mid `#6b5e52` · light `#c8c3b8` · accent
  `#d8532a`; desaturated cool with a warm accent.
- **Type**: Special Elite (titles) / Barlow / Barlow Condensed.
- **Shape**: worn, asymmetric, industrial proportions, visible damage.
- **Materials**: PBR with consistent texel density (512 px/m), roughness per material,
  decals for grime.
- **Lighting**: overcast or night presets, SSAO on, SDFGI optional, saturation 0.9, fog
  medium.
- **Asset tags**: `realistic`, `pbr`, `industrial`, `worn`, `military`, `urban`.
- **Godot**: `LINEAR_WITH_MIPMAPS_ANISOTROPIC`, shadows high, ACES tonemap.
- **Never mix**: toy proportions, cel outlines, pixel, neon pastel.

<!-- section: clay-render -->
## clay-render
- **Mood**: tactile, calm, pastel. Archetypes: puzzle physics, cosy, kids.
- **Palette**: bg `#e9e4de` · ground `#cfc6bc` · mid `#b39b8b` · light `#fbf8f4` · accent
  `#f28c6b`; warm greys.
- **Type**: Manrope 700 / Source Sans 3.
- **Shape**: soft, rounded, slightly imperfect, fingerprints allowed.
- **Materials**: matte clay, roughness 0.7, subsurface scattering low, no textures.
- **Lighting**: overcast preset, soft shadows (blur 3), SSAO on, no glow.
- **Asset tags**: `clay`, `soft`, `rounded`, `matte`, `pastel`.
- **Godot**: `LINEAR`, filmic tonemap, exposure 1.1.
- **Never mix**: hard outlines, neon, pixel, metals.

<!-- section: voxel -->
## voxel
- **Mood**: blocky, buildable, cheerful. Archetypes: survival, builder, exploration.
- **Palette**: bg `#5aa9e6` · ground `#6f9b3f` · mid `#8b6b4a` · light `#f2f2e6` · accent
  `#f2b134`.
- **Type**: Press Start 2P (titles) / Pixelify Sans / VT323.
- **Shape**: cubes on a 0.25 m or 1 m grid; no diagonals; characters 2–3 cubes tall.
- **Materials**: per-face flat colours or 16 px textures, `NEAREST`, roughness 0.9.
- **Lighting**: clear noon, hard shadows, SSAO low.
- **Asset tags**: `voxel`, `cube`, `blocky`, `magicavoxel`.
- **Godot**: `NEAREST`, greedy-meshed voxel scenes, integer grid snap.
- **Never mix**: smooth meshes, painterly, cel outlines.

<!-- section: retro-ps1 -->
## retro-ps1
- **Mood**: uneasy, low-fi, nostalgic. Archetypes: horror, FPS, survival.
- **Palette**: bg `#101018` · ground `#3c3c4c` · mid `#7a6a5a` · light `#d8d0c0` · accent
  `#c83c3c`.
- **Type**: VT323 (titles and HUD) / Barlow.
- **Shape**: low poly with visible facets, affine-warped textures, vertex snapping.
- **Materials**: 64–128 px textures, `NEAREST`, unlit or vertex-lit, dithering.
- **Lighting**: night or interior presets, fog dense and near, no SSAO.
- **Asset tags**: `ps1`, `retro`, `low-res`, `psx`, `dither`.
- **Godot**: `NEAREST`, a vertex-snap shader, render at 320 × 240 upscaled.
- **Never mix**: PBR, MSDF crisp fonts, glow, high-poly.

<!-- section: minimal-mono -->
## minimal-mono
- **Mood**: quiet, abstract, precise. Archetypes: puzzle, rhythm, meditative.
- **Palette**: bg `#f5f5f2` · ground `#e2e2dd` · mid `#9a9a94` · light `#ffffff` · accent
  `#111111`, one semantic hue only; or the dark inverse.
- **Type**: Zen Kaku Gothic New 500 / Noto Sans / Noto Sans Mono.
- **Shape**: primitives — spheres, cubes, planes; exact alignment; generous negative space.
- **Materials**: matte, roughness 0.9, one value per role, no textures.
- **Lighting**: overcast preset, soft long shadows, SSAO on, no fog.
- **Asset tags**: `minimal`, `abstract`, `primitive`, `mono`, `geometric`.
- **Godot**: `LINEAR`, filmic, saturation 0.95.
- **Never mix**: dressing clutter, textures, glow, more than one hue.
