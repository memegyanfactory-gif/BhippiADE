version: 1
domain: foundations
title: Space and layout
when: grid, proximity, alignment, gap over margin, hierarchy
tags: layout, spacing, grid, margin, padding, gap, flex, alignment, proximity, hierarchy, whitespace, column, breakpoint, container

# Space and layout

<!-- section: grid -->
## 1. The grid

A 4 px grid: `4 · 8 · 12 · 16 · 24 · 32 · 48 · 64`. Nothing between. The grid is what makes
unrelated components line up without anyone coordinating. In a 3D scene the equivalent is the
metric grid (`scene-3d/layout-metrics`); in a pixel game it is the pixel grid
(`scene-2d/sprites-tiles`).

<!-- section: proximity -->
## 2. Proximity does the grouping

Related things sit 4–8 px apart, groups 16–24 px apart, sections 32–64 px apart. A border
between two things that are already 24 px apart is a border doing nothing. Reach for a border
only when spacing genuinely cannot do it — a scrolling region, a table.

<!-- section: gap-not-margin -->
## 3. Let layout do the spacing

Lay out sibling groups with flex or grid and `gap`, not per-element margins that silently
collapse or double. Wide content — tables, code, diagrams — gets `overflow-x: auto` on its
own container so the page body never scrolls sideways. Text that can outgrow its track wraps
or scrolls in its own box; clipped text is a bug.

<!-- section: alignment -->
## 4. Align to something

Every edge shares an axis with another edge. Optical alignment beats mathematical where they
disagree: an icon sits a pixel high, a circle overshoots a square's edge, a triangle play
glyph shifts right. Baselines align across a row; the left edge of a label aligns with the
left edge of its control.

<!-- section: hierarchy -->
## 5. Three levels

Primary, secondary, tertiary — by size, weight, colour and position, in that order of
strength. A fourth level is invisible in practice. Surface the summary before the detail: on
a tool or dashboard the reader scans and operates, so what needs attention reads first.

<!-- section: repeated-objects -->
## 6. Compose repeated things as one object

Cards in a row, label/value pairs down a list, badges on siblings: same edges, baselines and
inner padding from one to the next, and a recurring element sits in the same place on each.
Let content set a container's height and pick a column count the items fill, so nothing
stretches over dead space or sits alone in a row.

<!-- section: shell -->
## 7. The shell

A tool shell: a fixed left rail (56 px collapsed / 240 px open), a content column, an
optional right panel. Prose maxes at 720–780 px. Split panels resize from a 4 px handle with
an 8 px hit area and remember their position. A reading page: one column, generous top
margin, the measure from `foundations/type#measure`. A game menu: one column of focusable
rows, centred or left-anchored, never a grid of tiles for fewer than six items.
