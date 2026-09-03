version: 1
domain: scene-3d
title: Materials and palette
when: palette per style, value ranges, roughness discipline
tags: material, palette, albedo, roughness, metallic, pbr, shader, vertex-color, texture, flat, toon, cel, outline, emissive, semantic, pickup, hazard, goal, 3d

# Materials and palette

<!-- section: palette -->
## 1. The scene palette comes from the brief

Five named values from `art-direction/brief`, mapped to the world: `ground` (the floor and
the large masses), `mid` (walls, trunks, structures), `light` (highlights, sky-facing
surfaces, snow, sand), `accent` (the one hero colour — a roof, a flag, a market awning), and
the **semantic three**: `pickup`, `hazard`, `goal`. Everything in the scene is one of these
or a tint of one. A prop that brings its own palette (a rainbow-coloured library asset in a
two-hue world) gets re-tinted through `albedo_color` or rejected in model selection.

<!-- section: value-ranges -->
## 2. Albedo value discipline

Albedo lightness (OKLCH L) stays between 0.25 and 0.85: nothing pure white (it blows out
under a key light and kills the tonemap), nothing pure black (it eats all shading). Large
masses sit in the middle (0.45–0.65), accents may go brighter and more saturated, semantic
colours are the most saturated things in the scene by design. Saturation on large surfaces
stays under 0.12 chroma; on accents up to 0.2.

<!-- section: roughness -->
## 3. Roughness and metallic

One material family per scene. Roughness is the texture of the style: toy and clay 0.6–0.8
(soft highlights), low-poly 0.7–0.9, painterly 0.9–1.0 (no specular), realistic per material
(wood 0.6, painted metal 0.4, wet stone 0.2). Metallic is 0 or 1, never in between, and 1
only for actual metal. Specular 0.5 default. A scene where everything is 0.3 roughness is
the plastic look; a scene where everything is 1.0 is chalk. `StandardMaterial3D` for
everything unless the style pack names a shader.

<!-- section: flat-and-toon -->
## 4. Flat, low-poly and toon

Low-poly and flat styles use **vertex colours or a tiny palette texture** (a 16 × 16 PNG
with one colour per cell, UVs pointed at cells), `shading_mode = PER_VERTEX` for the faceted
look or `PER_PIXEL` with `roughness 0.9` for smooth-flat; no normal maps, no detail
textures. Cel-shaded styles use a toon shader with two or three bands, a rim light in the
`light` value, and an outline (inverted hull at 0.02–0.04 m or a post pass), thickness
consistent across the scene. Pixel styles at 3D use `texture_filter = NEAREST` and a low
texture resolution per metre (16–32 px/m) held constant across every asset.

<!-- section: textures -->
## 5. Textures, when used

A consistent texel density across the scene (512 px/m for realistic, 128–256 for stylised);
a prop textured at four times its neighbour's density looks pasted in. Triplanar mapping for
terrain and large CSG. Tileable textures with visible repetition broken by a second material
or a decal every 8–10 m. Every texture has a licence sidecar.

<!-- section: semantic -->
## 6. Colour as a gameplay signal

`pickup`, `hazard` and `goal` are the only fully saturated colours in the scene, and they
never appear on dressing. A pickup glows slightly (`emission` at 0.3 in its own colour);
a hazard is its colour plus a shape (spikes, a stripe, a pulsing edge); the goal is its
colour plus light (a beam, a lantern). The three differ in hue *and* in brightness *and* in
shape, so a colour-blind player reads them.

<!-- section: emissive -->
## 7. Emissive

Emission is light the material claims to give off; it only reads under glow or in dark
scenes. Use it for the goal, for lamps, for neon in a neon style; `emission_energy` 1–3.
Never emissive white on a UI plate or a road line; never emissive on dressing.

<!-- section: checklist -->
## 8. Before calling the materials done

- every surface is one of the five palette roles or its tint
- albedo lightness inside 0.25–0.85; no pure white or black
- one roughness family; metallic 0 or 1
- semantic three are the only saturated colours; each differs in hue, brightness, shape
- texel density consistent; every texture licensed
