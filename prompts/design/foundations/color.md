version: 1
domain: foundations
title: Colour
when: ramps, the one accent, semantic colour, floors, chosen neutrals
tags: color, colour, palette, accent, neutral, ramp, contrast, semantic, token, oklch, hue, theme, dark, light, grey, gray

# Colour

A palette is **one accent over one neutral ramp**, plus semantic colours that are never used
decoratively. Everything here is a token; nothing is a literal in a component.

<!-- section: ramp -->
## 1. The neutral ramp

Seven steps, **one temperature**. Warm neutrals (a trace of yellow or red) read calm and
paper-like; cool neutrals (a trace of blue) read technical. Pick one per surface family and
never mix: a warm panel beside a cool one looks like a rendering bug.

| Token | Role | Rule |
|---|---|---|
| `--bg` | the page behind everything | darkest (dark theme) or lightest (light) |
| `--surface` | panels, cards, the composer | one step from `--bg` |
| `--surface-2` | recessed wells, inputs, hover | one step again |
| `--surface-3` | pressed, selected, active | the last step that is still neutral |
| `--line` | every hairline | visible against both `--surface` and `--surface-2` |
| `--line-strong` | emphasis borders, focus | about twice the contrast of `--line` |
| `--text` / `--text-dim` / `--text-faint` | primary / secondary / tertiary | 4.5:1 · 4.5:1 · 3:1 minimum |

<!-- section: chosen-neutrals -->
## 2. Choose neutrals; do not default to them

A pure mid-grey reads as unconsidered. A grey with a slight hue bias toward the accent (or
toward the subject — sea-green for a coastal game, ochre for a desert one) reads as chosen.
Pure white and near-black are fine grounds when the subject wants them; the point is that the
neutral was picked. In OKLCH terms: whites and blacks at chroma ≤ 0.02, greys at ≤ 0.03, with
the hue of the accent.

<!-- section: accent -->
## 3. The accent

One hue, five tokens: `--accent` (the colour), `--accent-hi` (hover), `--accent-dim` (a wash
at 12–18 % for backgrounds), `--accent-line` (a border at 35–45 %), and `--on-accent` (text
on the accent, which must itself clear 4.5:1). If the accent fights the ground, shift it
toward analogous or drop saturation rather than replacing it. If a subject wants two accents,
they share chroma and lightness and differ only in hue — and one of them is still the primary.

<!-- section: semantic -->
## 4. Semantic colours

`--ok`, `--warn`, `--error`, and in games `--hazard`, `--pickup`, `--goal`. Each means exactly
one thing and is never borrowed for emphasis. If a button is red, it deletes something. If a
crate glows the pickup colour, it can be picked up. Semantic colours always ship with a glyph
or a label; they are never the only signal.

<!-- section: contrast -->
## 5. Floors, non-negotiable

- Body text 4.5:1 · large text (≥ 18 px regular or ≥ 14 px bold) and UI glyphs 3:1.
- A focus indicator 3:1 against both the component and the page behind it.
- HUD text over a moving 3D scene: 4.5:1 against the *darkest and lightest* plausible
  backdrop, which in practice means a scrim, a plate or an outline — never bare text on the
  world.
- These are gates. A batch that names a pair under the floor is refused with the ratio.

<!-- section: both-themes -->
## 6. Both themes, three states

A page renders in the viewer's theme, and the viewer has three states: an explicit dark, an
explicit light, and "system", which stamps nothing. Define the complete light palette on the
bare `:root`; redefine only tokens under `@media (prefers-color-scheme: dark)` guarded as
`:root:not([data-theme="light"])`; redefine them again under `:root[data-theme="dark"]`.
Never give a colour its only definition inside a media or `[data-theme]` block, and give
`body` an explicit token background. The dark theme gets the same care as the light: shadows
soften, the accent may need a lighter step, saturated fills drop a notch. A design that
commits to one visual world (a neon arcade, a letterpress card) may stay single-theme, and
then still paints every colour explicitly.

<!-- section: chart-scale -->
## 7. Chart colours are a separate scale

Categorical series are assigned by identity, never by rank, and never drawn from the accent —
a chart tinted with the brand colour implies the brand *is* one of the series. See
`web/charts`.
