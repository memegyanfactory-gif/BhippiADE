version: 1
domain: web
title: Fonts
when: pairing method, vetted pairings by mood, loading, fallback stacks
tags: font, fonts, typeface, pairing, google-fonts, fallback, stack, loading, swap, variable, self-host, licence, mood, display, body, mono, serif, sans

# Fonts

<!-- section: method -->
## 1. The pairing method

1. Name the mood from the brief in two words (*warm editorial*, *cold instrument*, *playful
   toy*, *quiet luxury*, *retro arcade*).
2. Pick the **display** face for that mood — the one place personality is spent.
3. Pick a **body** face that contrasts in structure (serif ↔ sans, geometric ↔ humanist,
   condensed ↔ wide) and disappears when read.
4. Add a **utility** face only if data, code or captions exist: a mono or a compact grotesque
   with tabular figures.
5. Declare a fallback stack with close metrics for each, and check the page with fonts
   blocked once — it must still be the same design.

Never pair two faces that are almost the same; never use more than three; never reach for
Inter, Roboto, Arial, Space Grotesk or Fraunces as the "safe" choice when nothing is
specified.

<!-- section: pairings -->
## 2. Vetted pairings by mood

All available on Google Fonts. Display / body / utility.

| Mood | Display | Body | Utility |
|---|---|---|---|
| warm editorial | Newsreader (600) | Source Serif 4 | IBM Plex Mono |
| quiet luxury | Cormorant Garamond (500) | Lora | — |
| modern grotesque | Familjen Grotesk (700) | Instrument Sans | Geist Mono |
| cold instrument / lab | Chivo Mono (600) | Schibsted Grotesk | Chivo Mono |
| playful toy | Baloo 2 (800) | Nunito | — |
| storybook / cosy | Gaegu (700) | Atkinson Hyperlegible | — |
| retro arcade | Press Start 2P (title only) | Pixelify Sans | VT323 |
| sci-fi hard | Orbitron (700, title only) | Exo 2 | Share Tech Mono |
| fantasy / medieval | Cinzel (700) | Crimson Pro | — |
| horror / gritty | Special Elite (title only) | Barlow | Barlow Condensed |
| technical docs | Bricolage Grotesque (700) | Public Sans | JetBrains Mono |
| brutalist | Archivo Black | Archivo | Archivo Narrow |
| humanist product | Manrope (700) | Source Sans 3 | Fira Code |
| Japanese-inflected minimal | Zen Kaku Gothic New (500) | Noto Sans JP | Noto Sans Mono |

Rules: a pixel or display-only face never sets body text; a title-only face is used at
≥ 28 px and nowhere else; body faces are set at 16–18 px on reading pages and 13 px on tools.

<!-- section: loading -->
## 3. Loading

Google Fonts is the one external font host most sandboxes allow. Link it directly, one
request for all faces, with `display=swap`:

```html
<link rel="preconnect" href="https://fonts.googleapis.com">
<link rel="preconnect" href="https://fonts.gstatic.com" crossorigin>
<link rel="stylesheet" href="https://fonts.googleapis.com/css2?family=Newsreader:wght@600&family=Source+Serif+4:opsz,wght@8..60,400;8..60,600&display=swap">
```

Request only the weights used. A face from anywhere else is self-hosted: a `.woff2` in
`assets/fonts/` with a licence sidecar (OFL, Apache, or the vendor's terms verbatim), loaded
with `@font-face` and `font-display: swap`. An unlicensed font is an `unknown` asset and
blocks a Release export.

<!-- section: fallback -->
## 4. Fallback stacks with close metrics

The fallback must have a similar x-height and width so the layout does not jump when the web
font arrives:

| Web face | Stack |
|---|---|
| a humanist sans | `system-ui, "Segoe UI", Roboto, "Helvetica Neue", Arial, sans-serif` |
| a geometric sans | `"Avenir Next", "Trebuchet MS", system-ui, sans-serif` |
| a transitional serif | `Charter, "Iowan Old Style", Georgia, "Times New Roman", serif` |
| a mono | `ui-monospace, "Cascadia Mono", Consolas, "SF Mono", Menlo, monospace` |
| a display slab | `"Rockwell", "Roboto Slab", Georgia, serif` |

Tune with `size-adjust` and `ascent-override` in an `@font-face` fallback definition when
the jump is visible. Set `font-size-adjust` from the web face's aspect where supported.

<!-- section: variable -->
## 5. Variable fonts and optical sizes

Prefer a variable face where it exists: one file, every weight, `font-variation-settings`
for width and optical size. Set the optical size axis (`opsz`) to match the rendered size;
a 60 opsz serif at 13 px is spidery.

<!-- section: never -->
## 6. Never

- A display face for body text; a body face for a 96 px title.
- Text on the web in an image.
- Fake bold or fake italic (a weight the font does not ship).
- Uppercase running text; tracking below −0.02 em at body sizes.
- A font that is not declared in the plan.
