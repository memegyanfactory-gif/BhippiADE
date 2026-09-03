version: 1
domain: web
title: Charts
when: form before colour, the four colour jobs, the six checks, marks
tags: chart, graph, plot, dataviz, visualization, series, categorical, sequential, diverging, legend, axis, tooltip, sparkline, stat, kpi, palette, validator, minimap, meter

# Charts

A chart is read by people and executed by you. Colour comes **last**; most bad charts pick
colours first. The same method applies to a HUD meter, a minimap legend or a results screen.

<!-- section: form -->
## 1. Pick the form — sometimes not a chart

| The data is… | Use | Not |
|---|---|---|
| a single current value (+ a trend) | a stat tile (value, delta, sparkline) | a one-bar bar chart |
| a handful of headline numbers | a KPI row of stat tiles | a grouped bar chart |
| a single ratio against a limit | a meter on the same ramp | a two-slice pie |
| more than ~7 classes that all carry meaning | a table | more colours |

The job picks the type: compare magnitude → bar (heatmap for a grid); trend → line (area for
one series); tell series apart → grouped or stacked bar, multi-line; one series is the point
→ **emphasis** (one hue, the rest grey); above/below a baseline → diverging; part-to-whole →
stacked bar, horizontal for long names. Sequential is the safe default; categorical is for
when the series *are* the subject; emphasis is the most underused form.

<!-- section: colour-jobs -->
## 2. The four colour jobs

| Job | Encodes | Structure |
|---|---|---|
| categorical | identity | up to 8 hues in a **fixed order**, assigned in sequence, never cycled |
| ordinal | position in a sequence | one hue, monotone lightness steps |
| sequential | magnitude | one hue, light → dark; never a rainbow |
| diverging | polarity | two opposite hues (warm/cool) + a neutral grey midpoint |
| status | state | a reserved small scale, always with icon + label |

Colour follows the entity, never its rank: filtering out a series must not repaint the
survivors. Text wears text tokens, never the series colour. Status colours are never
borrowed for "series 4". Never draw series colours from the accent.

<!-- section: six-checks -->
## 3. The six checks — run them, never eyeball them

1. Fixed hue anchors in a fixed order.
2. Lightness band per mode (OKLCH L ≈ 0.43–0.77 light, 0.48–0.67 dark).
3. Chroma floor (C ≥ 0.10) — below it a hue reads as grey.
4. CVD separation: adjacent pairs ΔE ≥ 8 in OKLab×100 under simulated protanopia and
   deuteranopia (6–8 only with secondary encoding); a normal-vision floor of 15 is a hard
   fail; all pairs for scatter, bubble, maps and small multiples (which caps those at three
   series).
5. Contrast against the surface ≥ 3:1 for marks, relaxed only with visible labels or a table.
6. Documented palette only — every slot a value from the palette file.

The studio's `--series-1…8` in `ui/src/styles/tokens.css` is a validated instance; its order
is load-bearing. For a game's own palette, run the validator on the proposed hues before
they ship; a colour picked by eye is a guess.

<!-- section: marks -->
## 4. Marks and the two spacers

Bars ≤ 24 px thick, 4 px rounded at the data end, square at the baseline, from a single
baseline; lines 2 px with round joins; markers ≥ 8 px; area fills at ~10 % opacity; gridlines
and axes hairline, solid, one step off the surface, recessive. A **2 px surface gap** between
touching fills (stacked segments and adjacent bars alike) and a **2 px surface ring** on
overlapping markers do the separating — never a stroke around a mark. Label selectively: the
endpoint, the extreme, the one series that matters; a legend is always present for ≥ 2
series and absent for one. A label that does not fit moves outside or into the tooltip; it
is never clipped.

<!-- section: hover -->
## 5. Hover by default

An HTML chart is interactive: a crosshair and one tooltip listing every series at that X on
lines and areas; a per-mark tooltip on bars, dots and cells with the hovered mark lifted. Hit
targets are bigger than the marks (≥ 24 px). Tooltips enhance and never gate: every value is
reachable through labels or a table view, and keyboard focus gets the same readout. Insert
labels with `textContent`, never `innerHTML`.

<!-- section: anti-patterns -->
## 6. What goes wrong

Dual-axis charts (two y-scales — invent a correlation; use two charts or index to 100);
recolour on filter; a generated ninth hue; a value ramp on nominal categories; a hue at the
diverging midpoint; status colour for a series; eight hues when the story is one number; a
one-bar bar chart or a two-slice pie; a donut for close values; thick saturated blocks and
heavy grids; dashed gridlines; a number on every point; a border around marks; clipped
in-bar labels; a fixed-height container that hides the x-axis; a display face on the hero
figure; `tabular-nums` on a large standalone number.

<!-- section: in-games -->
## 7. In games

A health bar is a meter: one hue on its own ramp, a track one step off the surface, the
value in text beside it. A minimap legend is categorical with fixed slots (player, enemy,
objective, pickup) that never change meaning between levels. A results screen with a graph
(lap times, score over waves) follows the same form rule — usually a single-series line or a
stat row, never a rainbow of bars.
