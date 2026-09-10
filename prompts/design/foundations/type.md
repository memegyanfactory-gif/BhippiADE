version: 1
domain: foundations
title: Type
when: pairing by role, the scale, measure, weights, numerals
tags: type, typography, font, typeface, scale, weight, measure, line-height, numerals, tabular, heading, display, body, label

# Type

Typography carries the page even when the page is not about typography. It is the cheapest
place to be distinctive and the most common place to be generic.

<!-- section: pairing -->
## 1. Pair by role

Two roles minimum, three at most: a **display** face with character, used with restraint
(titles, the one hero line); a complementary **body** face built for reading; optionally a
**utility** face for captions, data and code (a mono or a compact grotesque). One family for
prose and one for code is a system; three unrelated families is a ransom note.

Pairing works on contrast of *structure*, not of mood: a high-contrast serif display over a
low-contrast sans body; a geometric display over a humanist body; a condensed display over a
wide body. Two faces that are almost the same read as a mistake. Concrete pairings, by mood,
with loading and fallback rules, live in `web/fonts`; Godot font practice in
`game-ui/godot-control#fonts`.

<!-- section: scale -->
## 2. Set a scale and stay on it

A dense tool (the studio, an editor, a dashboard):

| Token | px | Use |
|---|---|---|
| `--fs-micro` | 10 | keyboard hints, badges |
| `--fs-xs` | 11 | metadata, timestamps |
| `--fs-sm` | 12 | secondary UI, captions |
| `--fs-base` | 13 | everything by default |
| `--fs-md` | 15 | section headings |
| `--fs-lg` | 18 | screen titles |
| `--fs-xl` | 24 | the one hero line per screen |

A reading surface (a landing page, docs, a store page) starts at 16–18 px body and steps by a
ratio (1.2 for dense, 1.25–1.333 for editorial) — and stays on those steps. A game HUD is
sized for the viewing distance: see `game-ui/hud#readable-at-distance`.

<!-- section: measure -->
## 3. Measure and rhythm

Running text sits near 65 characters per line (45–75 is the band). A 1600 px line is
unreadable regardless of screen. Line height 1.5 for prose, 1.2 for headings, 1.6 for code.
Headings get `text-wrap: balance`; paragraphs `text-wrap: pretty` where supported. Uppercase
labels get a touch of letter-spacing (0.04–0.08 em) and never exceed 11–12 px.

<!-- section: weights -->
## 4. Weights

400 body · 500–550 emphasis · 600 headings. Nothing heavier at small sizes: 700 at 13 px is a
smudge. Weight is a hierarchy signal, so a screen uses at most three. Italic is for titles of
works and for a single foreign word, not for emphasis in UI.

<!-- section: numerals -->
## 5. Numerals

Numbers that change in place — timers, counters, scores, token counts — use
`font-variant-numeric: tabular-nums`, or the row shifts on every tick. A large standalone
number (a hero figure, a final score) uses proportional numerals — tabular spacing looks
gapped at 64 px. Right-align numeric columns.

<!-- section: loading -->
## 6. Fonts must actually load

A face that fails to load falls back silently and the design becomes a different design.
Every face declares a real fallback stack with close metrics; web pages load from Google
Fonts or self-host with a licence sidecar; Godot fonts are `.ttf`/`.otf` files in the project
with a sidecar. Details in `web/fonts#loading` and `game-ui/godot-control#fonts`. An unlicensed
font is an `unknown` asset and blocks a Release export.
