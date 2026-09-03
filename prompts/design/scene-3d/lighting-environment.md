version: 1
domain: scene-3d
title: Lighting and environment
when: key light, sky, temperature contrast, environment settings
tags: lighting, light, sun, directional, omni, spot, sky, ambient, shadow, fog, tonemap, ssao, glow, bloom, environment, worldenvironment, temperature, warm, cool, time-of-day, exposure

# Lighting and environment

Light is the cheapest way to make a scene look expensive and the most common way to make an
expensive one look flat.

<!-- section: key -->
## 1. One key light

One `DirectionalLight3D` is the sun or the moon: elevation 35–55° for a normal day (long
enough shadows to explain form), 15–25° for golden hour, 70°+ only for noon deserts.
Azimuth so that shadows fall across the main path, not along it. `light_energy` 1.0–1.5 for
day, 0.2–0.4 for night; `shadow_enabled` on; `directional_shadow_max_distance` to the level's
size, not the default; `shadow_blur` 1–2 for soft styles, 0.5 for hard ones. Only one
shadow-casting directional light exists.

<!-- section: temperature -->
## 2. Warm against cool

The key and the ambient contrast in temperature: a warm sun (`Color(1.0, 0.95, 0.85)`) over
a cool sky ambient (`Color(0.6, 0.7, 0.9)`), or a cool moon over warm lantern light. Same-
temperature key and ambient is the flat look. Fill comes from the sky, never from a second
sun; add an `OmniLight3D` or `SpotLight3D` only where the world has a lamp, in the lamp's
colour, with `shadow_enabled` off unless it is the scene's focal light.

<!-- section: sky-ambient -->
## 3. Sky and ambient

`WorldEnvironment` with a `ProceduralSkyMaterial` (or a `PanoramaSkyMaterial` for a
painted sky): sky top and horizon colours from the brief's palette, ground colour from the
darker neutral; `ambient_light_source = SKY`, `ambient_light_sky_contribution` 0.7–1.0,
`ambient_light_energy` 0.6–1.0. Reflected light source `SKY` so metals have something to
reflect. Night is a dark blue sky, never black.

<!-- section: fog -->
## 4. Fog for depth

Fog makes the three layers exist (`scene-3d/composition#layers`). `fog_enabled` on, colour
one step lighter than the sky at the horizon and tinted toward the sky, `fog_density`
0.002–0.01 for open worlds, higher indoors with a coloured light; `fog_sky_affect` 0.3–0.6
so the sky stays a sky. Volumetric fog only for a style that wants god rays and can afford
it (`volumetric_fog_density` 0.02–0.05, `volumetric_fog_albedo` tinted). Fog is the first
thing to check when a scene looks like a diorama.

<!-- section: tone-and-post -->
## 5. Tonemap and post

`tonemap_mode = FILMIC` (or `ACES` for realistic), `tonemap_exposure` 1.0, `white` 6.0.
**SSAO** on at low radius (1.0) and intensity (1.5–2.0) for every style except flat and
pixel — it is what grounds props on the floor. **Glow** only where the style says so
(neon, magic, arcade), `glow_intensity` 0.3–0.6, `glow_bloom` 0.1, HDR threshold 1.0, never
on UI. `ssil`/`sdfgi` for realistic styles only; low-poly and toy styles look better without.
Adjustments: `adjustment_saturation` 1.05–1.15 for toy and cartoon, 0.9 for gritty; never a
colour-correction gradient that fights the palette.

<!-- section: presets -->
## 6. Time-of-day presets

| Preset | Key colour / elevation | Sky top / horizon | Ambient | Fog |
|---|---|---|---|---|
| clear noon | `#fff5e6` / 60° | `#3d7fd6` / `#a9cbe8` | cool, 0.9 | light, `#b8d0e6` |
| golden hour | `#ffc27a` / 18° | `#4b6ea8` / `#f0a660` | warm, 0.7 | medium, `#e9b48a` |
| overcast | `#dfe4ea` / 45°, shadow blur 3 | `#8d99a6` / `#c5ccd3` | neutral, 1.0 | medium, `#c9d0d6` |
| night, moon | `#9fb3d9` / 40°, energy 0.3 | `#0e1a33` / `#243456` | cool, 0.4 | dense, `#1a2740` |
| interior, lamps | none; omni lights `#ffb870` | — | warm, 0.3 | slight |
| neon night | `#6a7cff` / 30°, energy 0.2 + glow | `#0b0b1a` / `#2a1a4a` | magenta 0.3 | dense, glow on |

Each preset is one set of `WorldEnvironment` and `DirectionalLight3D` properties, applied in
one batch, so *"make it evening"* is a parameter edit.

<!-- section: shadow-budget -->
## 7. Shadow budget

One directional shadow, up to four omni/spot shadows in view, everything else unshadowed.
`shadow_opacity` 0.8–0.9 so shadows are dark, not black; shadow colour comes from the
ambient, which is why the ambient must be tinted. Baked lightmaps for static interiors when
the style is realistic.

<!-- section: checklist -->
## 8. Before calling the lighting done

- one key light, shadows across the path, elevation matches the time of day
- key and ambient differ in temperature
- fog on; three layers read in the screenshot
- SSAO on (unless flat/pixel); glow only if the style says so
- shadows dark not black; no second sun
