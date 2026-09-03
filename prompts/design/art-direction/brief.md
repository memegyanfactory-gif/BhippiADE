version: 1
domain: art-direction
title: The art-direction brief
when: the brief every later decision is checked against
tags: art-direction, brief, mood, palette, style, reference, shape, language, material, lighting, camera, forbidden, plan-card, identity, look

# The art-direction brief

The brief is written once, at plan time, from the user's words and the archetype's
vocabulary, seeded from the nearest style pack. Every later decision — a HUD colour, a prop,
a light, a font — is checked against it. It is shown on the plan card and the user can edit
it before Approve; an edit is a stated preference and enters the taste profile.

<!-- section: schema -->
## 1. The schema

```
brief:
  subject:      "a cosy harbour town at dusk"          # one concrete world
  mood:         ["warm", "quiet", "storybook"]          # two or three words
  style_pack:   "low-poly-toy"                          # from art-direction/styles
  palette:                                              # five named values + the semantic three
    bg:      "#1f2a3a"   # the darkest large mass (sea, night sky)
    ground:  "#4a5a4a"
    mid:     "#8a6d4d"
    light:   "#f0dcc0"
    accent:  "#e26d3a"   # the one hero colour: roofs, lanterns
    pickup:  "#ffd23f"
    hazard:  "#ff4f5e"
    goal:    "#5ee0c8"
  type:
    display: "Baloo 2 800"        # title screen, level names
    body:    "Nunito 400/600"     # menus, dialogue
    ui:      "Nunito 600"         # HUD numbers, tabular
  shape_language: "rounded, chunky, no sharp corners; roofs and hulls are the same curve"
  materials:      "matte, roughness 0.75, vertex-coloured, no textures"
  lighting:       "golden hour preset, warm key over cool ambient, light fog"
  camera:         "third-person follow, 62° FOV, lower-third framing"
  references:     ["the user's attached sketch", "Kenney's nature kit"]
  forbidden:      ["neon", "gradients on UI", "photographic textures", "the colour purple"]
  subject_detail: "tide tables as the level-select screen"
```

<!-- section: writing -->
## 2. Writing it

- **Subject** first: a place, a material world, a time. Not a genre.
- **Mood** in two or three words that a painter would use, not marketing words.
- **Style pack** is the nearest one from the catalogue; the brief may override any of its
  fields, and the override is stated.
- **Palette**: pick with the neutral's temperature stated; check the semantic three differ
  in hue, brightness and (later) shape; run the chart validator on any categorical set the
  game will need (a minimap legend).
- **Type** from `web/fonts#pairings` by mood; the display face is the only flourish.
- **Shape language** is one sentence that a modeller could follow: what curves, what
  angles, what proportions repeat.
- **Forbidden** is the most useful line: it is where the user's taste enters first, and it
  is what model selection's never-mix check reads.
- **Subject detail**: the one thing only this world would have, carried as content.

<!-- section: using -->
## 3. Using it

Every batch that touches a visible thing cites the brief: a material's albedo is a palette
role or its tint; a font is one of the three; a light is the named preset; a prop's style
tags come from the pack; a forbidden word in a candidate's tags scores it out. When a
decision has to leave the brief (the user asks for a purple boss), the brief is edited, not
ignored — the edit is a stated preference and is visible on the plan card.

<!-- section: drift -->
## 4. Drift

The critique's `drift` field compares the shipped surface to the brief. A drift is not a
failure by itself; it is shown to the user, who either accepts it (and the brief updates) or
sends it back. Two drifts in the same direction become a proposed lesson.

<!-- section: per-archetype -->
## 5. Seeds per archetype

| Archetype | Default pack | Mood seed | Notes |
|---|---|---|---|
| platformer 3D | `low-poly-toy` | bright, bouncy | rounded shapes, saturated accent |
| platformer 2D | `pixel-16` | crisp, nostalgic | 16 px tiles, 32-colour palette |
| top-down action | `flat-vector` | clean, readable | strong silhouettes, flat light |
| FPS arena | `gritty-realistic` or `retro-ps1` | hard, tense | cool palette, harsh key |
| exploration | `painterly-soft` | calm, vast | fog, warm/cool layers |
| racing kart | `cel-shaded` | loud, fast | high saturation, outlines |
| puzzle physics | `clay-render` | tactile, calm | soft shadows, pastel |
| tower defence | `flat-vector` | tidy, tactical | isometric, clear lanes |
| survival | `gritty-realistic` | cold, sparse | desaturated, night presets |
| endless runner | `neon-arcade` | electric | glow on, dark ground |
