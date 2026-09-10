version: 1
domain: foundations
title: Icons and imagery
when: icons on a grid, never emoji; illustration and photo rules
tags: icon, svg, glyph, illustration, image, photo, imagery, logo, sprite-sheet, favicon, texture, generative, canvas

# Icons and imagery

<!-- section: icons -->
## 1. Icons

Never emoji or dingbat glyphs as icons. Draw or pick stroke-based SVG on a 16 / 20 / 24 px
grid, one consistent stroke weight (1.5 px at 16, 2 px at 24), rounded or square caps chosen
once. An icon-only control has a label for assistive tech and a tooltip for everyone else.
Icons take `currentColor` so they recolour with their text. In Godot, icons are `SVG`
imported at the sizes used (or an MSDF font of glyphs), never a bitmap scaled up.

<!-- section: illustration -->
## 2. Illustration

One style per product: same line weight, same palette from the tokens, same level of detail.
An illustration earns its place on a surface seen once (empty state, onboarding, a title
card), not beside every paragraph. A spot illustration is smaller than the text it
accompanies.

<!-- section: photos -->
## 3. Photographs and renders

Consistent treatment: same crop ratio, same colour grade, a scrim or a plate before any text
sits on them. A screenshot in a store page is real, current and at the game's aspect ratio.
Never place text on a busy image without a plate; never stretch an image; never use a
photograph where the subject's world is drawn.

<!-- section: generative -->
## 4. Generative and decorative graphics

Backgrounds with atmosphere — gradient meshes, noise, grain, patterns, layered transparencies
— are drawn with Canvas or WebGL (or a Godot shader), not hand-authored SVG path data. They
sit behind content at low contrast and never move on a surface seen often.

<!-- section: marks -->
## 5. Marks and favicons

A game's icon reads at 32 px: one shape, two colours, no text. A favicon or app icon is the
same mark, not the logo shrunk. The mark stays the same for the life of the product; people
find things by it.
